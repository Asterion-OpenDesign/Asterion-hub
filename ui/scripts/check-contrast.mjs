// Fails the build when a text/background token pair drops under its WCAG 2.x floor.
// Reads the real tokens.css, so the check cannot drift from the stylesheet.
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

const css = readFileSync(fileURLToPath(new URL('../src/theme/tokens.css', import.meta.url)), 'utf8')

function block(selector) {
  const start = css.indexOf(selector + ' {')
  if (start < 0) throw new Error('missing block ' + selector)
  const end = css.indexOf('\n}', start)
  const vars = {}
  for (const m of css.slice(start, end).matchAll(/--([a-z0-9-]+):\s*(#[0-9a-fA-F]{6})/g)) vars[m[1]] = m[2]
  return vars
}

const light = block(':root')
const dark = { ...light, ...block("[data-theme='dark']") }

const lum = (hex) => {
  const c = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4))
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}
const ratio = (a, b) => {
  const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p)
  return (x + 0.05) / (y + 0.05)
}

const surfaces = ['papel', 'sup-sutil', 'sup-campo', 'sup-hover']
const text = ['tinta', 'tinta-2', 'tinta-3']
const pairs = []
for (const t of text) for (const s of surfaces) pairs.push([t, s, 4.5])
for (const s of surfaces) pairs.push(['marca', s, 4.5])
pairs.push(['sobre-tinta', 'fondo-tinta', 4.5], ['sobre-tinta', 'fondo-tinta-hover', 4.5])
pairs.push(['marca', 'marca-lavado', 4.5])
for (const k of ['ok', 'rev', 'fdr']) pairs.push([`${k}-tinta`, `${k}-campo`, 4.5])
// functional borders (what says where a control starts) need 3:1 against the surfaces they sit on
for (const s of ['papel', 'sup-sutil']) pairs.push(['filete-control', s, 3])

let bad = 0
for (const [name, theme] of [['light', light], ['dark', dark]]) {
  for (const [fg, bg, min] of pairs) {
    const r = ratio(theme[fg], theme[bg])
    if (r < min) {
      bad++
      console.error(`FAIL ${name}: ${fg} ${theme[fg]} on ${bg} ${theme[bg]} = ${r.toFixed(2)}:1 (needs ${min}:1)`)
    }
  }
}
if (bad) process.exit(1)
console.log(`contrast OK: ${pairs.length} pairs x 2 themes`)
