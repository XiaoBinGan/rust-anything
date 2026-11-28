use std::path::{Path, PathBuf};

use rust_anything::lang::{Lang, OVERLOADED};
use rust_anything::{analyze, bridge};

const CONFIG: &str = r#"package mini;

public final class Config {
    private static Config sInstance;
    private final String locale, visitor;
    private Config(String locale, String visitor) { this.locale = locale; this.visitor = visitor; }
    public static Config instance() {
        if (sInstance == null) sInstance = new Config("en", "visitor-1");
        return sInstance;
    }
    public String getLocale() { return locale; }
    public String getVisitor() { return visitor; }
}
"#;

const QUERY: &str = r#"package mini;

public final class Query {
    private Query() { }

    public static String getContinuationQuery(String nextPageKey) {
        String continuation = String.format("\"continuation\":\"%s\"", nextPageKey);
        return "{\"context\":{\"client\":{\"hl\":\"" + Config.instance().getLocale()
                + "\",\"visitorData\":\"" + Config.instance().getVisitor() + "\"}}," + continuation + "}";
    }
}
"#;

fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "rust-anything-state-through-calls-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in files {
        let full = root.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }
    root
}

/// Bridges `unit` of `lang` and returns the bridged export names and the kept names with their reasons.
fn bridge_unit(lang: Lang, root: &Path, unit: &str) -> anyhow::Result<(Vec<String>, Vec<String>)> {
    let out = root.join("port");
    let outcome = bridge::bridge(&bridge::Options {
        root,
        unit,
        kind: None,
        out: Some(out.clone()),
        name: Some("mini-rs".into()),
        force: false,
        keep: Vec::new(),
        lang: Some(lang),
        batch: Vec::new(),
        as_json: Vec::new(),
        allocator: Default::default(),
    })?;
    let manifest = bridge::Manifest::load(&out)?;
    Ok((
        outcome.exports,
        manifest
            .js_only
            .into_iter()
            .map(|j| format!("{}: {}", j.name, j.reason))
            .collect(),
    ))
}

#[test]
fn a_function_that_reaches_app_state_through_another_stays_in_java() {
    let root = project(
        "repro",
        &[("mini/Config.java", CONFIG), ("mini/Query.java", QUERY)],
    );
    let err = bridge_unit(Lang::Java, &root, "mini")
        .unwrap_err()
        .to_string();
    let report = analyze::analyze_lang(&root, Lang::Java).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    assert!(err.contains("instance (uses the class `Config`)"), "{err}");
    assert!(
        err.contains("getContinuationQuery (calls `instance`, which uses the class `Config`)"),
        "{err}"
    );
    let unit = report
        .units
        .iter()
        .find(|u| u.id.trim_end_matches('/') == "mini")
        .unwrap();
    assert_eq!(unit.bridgeable, 0);
}

#[test]
fn the_chain_is_followed_and_the_pure_rest_is_bridged() {
    let query = r#"package mini;

public final class Query {
    public static String getContinuationQuery(String nextPageKey) {
        return Config.instance().getLocale() + nextPageKey;
    }
    public static String page(String key) { return "page:" + getContinuationQuery(key); }
    public static int twice(int n) { return n * 2; }
}
"#;
    let root = project(
        "chain",
        &[("mini/Config.java", CONFIG), ("mini/Query.java", query)],
    );
    let result = bridge_unit(Lang::Java, &root, "mini");
    std::fs::remove_dir_all(&root).unwrap();
    let (exports, kept) = result.unwrap();
    assert_eq!(exports, ["twice"]);
    assert_eq!(
        kept,
        [
            "instance: uses the class `Config`",
            "getContinuationQuery: calls `instance`, which uses the class `Config`",
            "page: calls `getContinuationQuery`, which calls `instance`, which uses the class \
             `Config`",
        ]
    );
}

#[test]
fn a_call_to_a_function_kept_for_its_signature_alone_does_not_keep_the_caller() {
    let source = r#"package mini;

public final class Pad {
    public static String pad(String s, int n) { return s; }
    public static String pad(String s) { return pad(s, 8); }
    public static String title(String s) { return pad(s).toUpperCase(); }
}
"#;
    let root = project("overload", &[("mini/Pad.java", source)]);
    let result = bridge_unit(Lang::Java, &root, "mini");
    std::fs::remove_dir_all(&root).unwrap();
    let (exports, kept) = result.unwrap();
    assert_eq!(exports, ["title"]);
    assert_eq!(kept, [format!("pad: {OVERLOADED}")]);
}

#[test]
fn a_kotlin_function_that_reaches_an_object_through_another_stays_in_kotlin() {
    let config = "package mini\n\nclass Config(val locale: String)\n\nobject Settings {\n    \
                  @JvmStatic\n    fun instance(): Config { return Config(\"en\") }\n}\n";
    let query = "package mini\n\nfun continuationQuery(key: String): String {\n    \
                 return Settings.instance().locale + key\n}\n\n\
                 fun twice(n: Int): Int {\n    return n * 2\n}\n";
    let root = project(
        "kotlin",
        &[("mini/Config.kt", config), ("mini/Query.kt", query)],
    );
    let result = bridge_unit(Lang::Kotlin, &root, "mini");
    std::fs::remove_dir_all(&root).unwrap();
    let (exports, kept) = result.unwrap();
    assert_eq!(exports, ["twice"]);
    assert_eq!(
        kept,
        [
            "instance: uses the class `Config`",
            "continuationQuery: calls `instance`, which uses the class `Config`",
        ]
    );
}
