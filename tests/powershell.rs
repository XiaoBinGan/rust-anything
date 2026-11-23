//! PowerShell units: bridge, record, build, diff, gate, enable and disable on a `.psm1` module,
//! through `pwsh` (skipped without PowerShell 7.4 or newer).

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use rust_anything::bridge::{self, Kind, Manifest};
use rust_anything::harness::{self, DiffOptions, RecordOptions};

const SIGNATURES: [&str; 6] = [
    "pub fn convert_to_slug(text: String, separator: String, max_length: i64) -> Result<String, String>",
    "pub fn get_word_count(text: String) -> Result<serde_json::Value, String>",
    "pub fn measure_mean(values: Option<Vec<f64>>) -> Result<f64, String>",
    "pub fn get_initials(names: Option<Vec<String>>, upper: bool) -> Result<serde_json::Value, String>",
    "pub fn test_equal(a: String, b: String) -> Result<bool, String>",
    "pub fn get_setting(\n    settings: Option<serde_json::Value>,\n    name: String,\n    default: Option<serde_json::Value>,\n) -> Result<serde_json::Value, String>",
];

const PORT: &str = r#"//! The Rust port of `TextTools.psm1`.

#![forbid(unsafe_code)]

use serde_json::Value;

const NULL_CALL: &str = "System.Management.Automation.RuntimeException: You cannot call a method on a null-valued expression.";

fn lower(c: char) -> char {
    let mut low = c.to_lowercase();
    match (low.next(), low.next()) {
        (Some(l), None) if c != '\u{130}' => l,
        _ => c,
    }
}

fn upper(c: char) -> char {
    let mut up = c.to_uppercase();
    match (up.next(), up.next()) {
        (Some(u), None) if c != '\u{131}' && c != '\u{17f}' => u,
        _ => c,
    }
}

fn caller_view(v: Value) -> Value {
    match v {
        Value::Array(mut items) if items.len() < 2 => items.pop().unwrap_or(Value::Null),
        other => other,
    }
}

pub fn convert_to_slug(text: String, separator: String, max_length: i64) -> Result<String, String> {
    let lowered: String = text.chars().map(lower).collect();
    let mut replaced = String::new();
    let mut in_run = false;
    for c in lowered.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '\u{130}' {
            replaced.push(c);
            in_run = false;
        } else if !in_run {
            replaced.push_str(&separator);
            in_run = true;
        }
    }
    let slug = replaced.trim_matches(|c| separator.contains(c)).to_string();
    let units: Vec<u16> = slug.encode_utf16().collect();
    match usize::try_from(max_length) {
        Ok(max) if max > 0 && units.len() > max => Ok(String::from_utf16_lossy(&units[..max])),
        _ => Ok(slug),
    }
}

pub fn get_word_count(text: String) -> Result<Value, String> {
    let mut counts: Vec<(String, String, i64)> = Vec::new();
    for word in text.split(char::is_whitespace) {
        if word.is_empty() {
            continue;
        }
        let key: String = word.chars().map(lower).collect();
        let folded: String = key.chars().map(upper).collect();
        match counts.iter_mut().find(|(_, f, _)| *f == folded) {
            Some(entry) => {
                entry.0 = key;
                entry.2 += 1;
            }
            None => counts.push((key, folded, 1)),
        }
    }
    Ok(Value::Object(
        counts
            .into_iter()
            .map(|(k, _, n)| (k, Value::from(n)))
            .collect(),
    ))
}

pub fn measure_mean(values: Option<Vec<f64>>) -> Result<f64, String> {
    let values = values.unwrap_or_default();
    if values.is_empty() {
        return Err("System.ArgumentException: mean of nothing".into());
    }
    let mut sum = 0.0;
    for v in &values {
        sum += v;
    }
    Ok(sum / values.len() as f64)
}

pub fn get_initials(names: Option<Vec<String>>, upper: bool) -> Result<Value, String> {
    let mut out = Vec::new();
    for n in names.unwrap_or_default() {
        if let Some(first) = n.encode_utf16().next() {
            out.push(String::from_utf16_lossy(&[first]));
        }
    }
    if upper {
        out = out
            .into_iter()
            .map(|s| s.chars().map(self::upper).collect())
            .collect();
    }
    Ok(caller_view(Value::from(out)))
}

pub fn test_equal(a: String, b: String) -> Result<bool, String> {
    let fold = |s: &str| s.chars().map(upper).collect::<String>();
    Ok(fold(&a) == fold(&b))
}

pub fn get_setting(
    settings: Option<Value>,
    name: String,
    default: Option<Value>,
) -> Result<Value, String> {
    let Some(Value::Object(settings)) = settings else {
        return Err(NULL_CALL.into());
    };
    Ok(caller_view(match settings.get(&name) {
        Some(v) => v.clone(),
        None => default.unwrap_or(Value::Null),
    }))
}
"#;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/powershell-module")
}

fn options<'a>(root: &'a Path, out: &Path, batch: Vec<String>) -> bridge::Options<'a> {
    bridge::Options {
        root,
        unit: "TextTools.psm1",
        kind: None,
        out: Some(out.to_path_buf()),
        name: Some("texttools-rs".into()),
        force: false,
        keep: Vec::new(),
        lang: None,
        batch,
        as_json: Vec::new(),
        allocator: Default::default(),
    }
}

fn diff(dir: &Path) -> harness::DiffOutcome {
    harness::diff(&DiffOptions {
        crate_dir: dir,
        module: None,
        ignore_key_order: false,
        float_tolerance: None,
        node_args: &[],
    })
    .unwrap()
}

fn pwsh(script: &str) -> String {
    let out = Command::new("pwsh")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n")
}

#[test]
fn a_module_is_bridged_with_its_parameter_blocks() {
    let dir = std::env::temp_dir().join(format!("rust-anything-ps-bridge-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let batch = bridge::bridge(&options(
        &fixture(),
        &dir,
        vec!["Measure-Mean:Values".into()],
    ))
    .unwrap_err()
    .to_string();
    assert!(batch.contains("not available for PowerShell"), "{batch}");
    let outcome = bridge::bridge(&options(&fixture(), &dir, Vec::new())).unwrap();
    assert_eq!(outcome.kind, Kind::Ffi);
    let manifest = Manifest::load(&dir).unwrap();
    let kept: Vec<(&str, &str)> = manifest
        .js_only
        .iter()
        .map(|j| (j.name.as_str(), j.reason.as_str()))
        .collect();
    assert_eq!(
        kept,
        [
            ("Get-Lines", "takes pipeline input `$Line`"),
            ("Format-Stamp", "takes a .NET object `$At` (`[datetime]`)"),
            ("Invoke-Twice", "takes a script block `$Action`"),
        ]
    );
    let port = std::fs::read_to_string(dir.join("src/port.rs")).unwrap();
    for signature in SIGNATURES {
        assert!(port.contains(signature), "{signature}\n{port}");
    }
    let wrapper = std::fs::read_to_string(dir.join("ps/texttools_rs.ps1")).unwrap();
    assert!(
        wrapper.contains(
            "function ConvertTo-Slug {\n    [CmdletBinding()]\n    [OutputType([string])]\n    param(\n        \
             [Parameter(Mandatory)]\n        [string]$Text,"
        ),
        "{wrapper}"
    );
    assert!(!wrapper.contains("function Get-Lines"), "{wrapper}");
    let refused = rust_anything::shadow::shadow(&dir, None)
        .unwrap_err()
        .to_string();
    assert!(refused.contains("rust-anything enable"), "{refused}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_module_records_diffs_gates_and_switches_on() {
    if !common::have_with(
        "pwsh",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "exit [int]($PSVersionTable.PSVersion -lt [version]'7.4')",
        ],
        "7.4 or newer was not found",
    ) {
        return;
    }
    let base = std::env::temp_dir().join(format!("rust-anything-ps-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let app = base.join("app");
    std::fs::create_dir_all(&app).unwrap();
    let original = std::fs::read_to_string(fixture().join("TextTools.psm1")).unwrap();
    std::fs::write(app.join("TextTools.psm1"), &original).unwrap();
    let dir = base.join("texttools-rs");
    bridge::bridge(&options(&app, &dir, Vec::new())).unwrap();

    let inputs = dir.join("inputs.jsonl");
    std::fs::write(
        &inputs,
        "{\"fn\": \"Get-WordCount\", \"args\": [\"to be or TO be\"]}\n\
         {\"fn\": \"Measure-Mean\", \"args\": [[]]}\n\
         {\"fn\": \"Get-Initials\", \"args\": [[\"ann\", \"bob\"], true]}\n\
         {\"fn\": \"Get-Initials\", \"args\": [[\"ann\"], false]}\n\
         {\"fn\": \"Test-Equal\", \"args\": [\"Straße\", \"STRASSE\"]}\n\
         {\"fn\": \"Test-Equal\", \"args\": [\"Hello\", \"hELLO\"]}\n\
         {\"fn\": \"ConvertTo-Slug\", \"args\": [\"\", \"-\", 0]}\n\
         {\"fn\": \"ConvertTo-Slug\", \"args\": [\"Hello, World\", {\"$undefined\": true}, 8]}\n",
    )
    .unwrap();
    let recorded = harness::record(&RecordOptions {
        crate_dir: &dir,
        count: 30,
        inputs: Some(&inputs),
        module: None,
        resolve_from: None,
        node_args: &[],
    })
    .unwrap();
    assert!(recorded.cases > 100, "{recorded:?}");
    let cases = std::fs::read_to_string(dir.join("cases.jsonl")).unwrap();
    for line in [
        r#"{"fn":"Get-WordCount","args":["to be or TO be"],"ok":{"to":2,"be":2,"or":1}}"#,
        r#"{"fn":"Measure-Mean","args":[[]],"error":"mean of nothing","error_type":"System.ArgumentException"}"#,
        r#"{"fn":"Get-Initials","args":[["ann","bob"],true],"ok":["A","B"]}"#,
        r#"{"fn":"Get-Initials","args":[["ann"],false],"ok":"a"}"#,
        r#"{"fn":"Test-Equal","args":["Hello","hELLO"],"ok":true}"#,
        r#"{"fn":"ConvertTo-Slug","args":["Hello, World",{"$undefined":true},8],"ok":"hello-wo"}"#,
    ] {
        assert!(cases.lines().any(|l| l == line), "{line}\n{cases}");
    }
    assert!(
        cases.contains(
            r#""error_type":"System.Management.Automation.ParameterBindingValidationException""#
        ),
        "an empty mandatory string fails to bind, in the original and in the wrapper alike"
    );

    let port = if pwsh("[string]([char]0x130 -match '^[a-z]$')") == "True\n" {
        PORT.to_string()
    } else {
        let invariant = PORT.replace(" || c == '\\u{130}'", "");
        assert_ne!(invariant, PORT);
        invariant
    };
    std::fs::write(dir.join("src/port.rs"), &port).unwrap();
    harness::build(&dir).unwrap();
    let outcome = diff(&dir);
    assert_eq!(outcome.total, recorded.cases);
    assert_eq!(outcome.passed, outcome.total, "{:?}", outcome.mismatches);

    std::fs::write(
        dir.join("src/port.rs"),
        port.replace("Ok(fold(&a) == fold(&b))", "Ok(a == b)"),
    )
    .unwrap();
    harness::build(&dir).unwrap();
    let broken = diff(&dir);
    assert!(broken.failed > 0);
    assert!(
        broken
            .mismatches
            .iter()
            .all(|m| m["fn"].as_str() == Some("Test-Equal")),
        "{:?}",
        broken.mismatches
    );
    std::fs::write(dir.join("src/port.rs"), &port).unwrap();

    let gate = rust_anything::gate::gate(&dir, &[]).unwrap();
    assert!(gate.passed, "{:?}", gate.checks);

    let check = format!(
        "Import-Module '{}' -Force -DisableNameChecking; \
         [string]((Get-Command ConvertTo-Slug).Definition -match 'RustAnythingCodec'); \
         ConvertTo-Slug -Text 'Hello World' -MaxLength 8; \
         (Get-Initials -Names 'ann', 'bob' -Upper) -join ','",
        app.join("TextTools.psm1").display()
    );
    let switched = rust_anything::enable::enable(&dir).unwrap();
    assert_eq!(switched.functions.len(), 6, "{switched:?}");
    let enabled = std::fs::read_to_string(app.join("TextTools.psm1")).unwrap();
    let block = enabled
        .find(">>> rust-anything: texttools-rs is on")
        .unwrap();
    assert!(
        block > enabled.find("function Invoke-Twice").unwrap()
            && block < enabled.find("Export-ModuleMember").unwrap(),
        "{enabled}"
    );
    assert_eq!(pwsh(&check), "True\nhello-wo\nA,B\n");
    let original_run = Command::new("pwsh")
        .args(["-NoProfile", "-NonInteractive", "-Command", &check])
        .env("RUST_ANYTHING_UNIT_TEXTTOOLS", "original")
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&original_run.stdout).replace("\r\n", "\n"),
        "False\nhello-wo\nA,B\n"
    );
    std::fs::write(
        dir.join("src/port.rs"),
        port.replace("Ok(fold(&a) == fold(&b))", "Ok(a == b)"),
    )
    .unwrap();
    harness::build(&dir).unwrap();
    harness::record(&RecordOptions {
        crate_dir: &dir,
        count: 30,
        inputs: Some(&inputs),
        module: None,
        resolve_from: None,
        node_args: &[],
    })
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("cases.jsonl")).unwrap(),
        cases,
        "an enabled unit records the original, not the (broken) port"
    );
    rust_anything::enable::disable(&dir).unwrap();
    assert_eq!(
        std::fs::read_to_string(app.join("TextTools.psm1")).unwrap(),
        original
    );
    let _ = std::fs::remove_dir_all(&base);
}
