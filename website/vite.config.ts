import { readdirSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vite-plus'
import { voidPlugin } from 'void'
import { voidReact } from '@void/react/plugin'
import { voidMarkdown } from '@void/md/plugin'
import tailwindcss from '@tailwindcss/vite'
import { mdIncludePlugin } from './scripts/md-include.mjs'
import {
  faviconPlugin,
  llmsFullPlugin,
  notFoundPlugin,
  ogImagePlugin,
  searchIndexPlugin,
} from './scripts/build-plugins.mjs'
import { allDocs, groupOf } from './lib/nav.ts'

const root = fileURLToPath(new URL('.', import.meta.url)).replace(/\/$/, '')

function docPages(dir: string, base: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const full = join(dir, name)
    if (statSync(full).isDirectory()) return docPages(full, `${base}/${name}`)
    return name.endsWith('.md') ? [`${base}/${name.slice(0, -3)}`] : []
  })
}

const translatedDocs = readdirSync(join(root, 'pages'))
  .filter((loc) => loc !== 'en' && statSync(join(root, 'pages', loc, 'docs'), { throwIfNoEntry: false })?.isDirectory())
  .flatMap((loc) => docPages(join(root, 'pages', loc, 'docs'), `/${loc}/docs`))

export default defineConfig({
  define: {
    __TRANSLATED_DOCS__: JSON.stringify(translatedDocs),
  },
  resolve: {
    alias: [{ find: /^@\//, replacement: `${root}/` }],
  },
  server: {
    fs: { allow: ['..'] },
  },
  plugins: [
    faviconPlugin(),
    tailwindcss(),
    voidPlugin(),
    voidReact(),
    mdIncludePlugin(),
    voidMarkdown(),
    searchIndexPlugin(),
    ogImagePlugin((route: string) => groupOf(route)?.title ?? ''),
    notFoundPlugin(),
    llmsFullPlugin(allDocs.map((d) => d.path)),
  ],
})
