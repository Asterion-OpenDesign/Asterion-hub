// Fails the build when a foreground/background token pair drops under its WCAG 2.x floor.
// Reads the real tokens.css and composites the translucent planes over the floor, so the check cannot
// drift from the stylesheet and measures against the colour that is actually painted.
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

const css = readFileSync(fileURLToPath(new URL('../src/theme/tokens.css', import.meta.url)), 'utf8')

function block(header) {
  const start = css.indexOf(header)
  if (start < 0) throw new Error('missing block ' + header)
  const end = css.indexOf('\n}', start)
  const vars = {}
  for (const m of css.slice(start, end).matchAll(/--([a-z0-9-]+):\s*(#[0-9a-fA-F]{6}|rgba\([^)]*\))/g)) vars[m[1]] = m[2]
  return vars
}

const dark = block(':root,\n[data-theme=\'dark\'] {')
const light = { ...dark, ...block("[data-theme='light'] {") }

const rgb = (v) => {
  if (v.startsWith('#')) return { c: [1, 3, 5].map((i) => parseInt(v.slice(i, i + 2), 16)), a: 1 }
  const m = v.match(/rgba\(([^)]*)\)/)[1].split(',').map((s) => parseFloat(s))
  return { c: m.slice(0, 3), a: m[3] }
}
const over = (top, under) => {
  const t = rgb(top)
  return t.c.map((x, i) => Math.round(x * t.a + under[i] * (1 - t.a)))
}
const lum = (c) => {
  const [r, g, b] = c.map((v) => v / 255).map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4))
  return 0.2126 * r + 0.7152 * g + 0.0722 * b
}
const ratio = (a, b) => {
  const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p)
  return (x + 0.05) / (y + 0.05)
}

let bad = 0
let checked = 0
for (const [name, t] of [['dark', dark], ['light', light]]) {
  const floor = rgb(t.noche).c
  const plano = over(t['lavado-1'], floor)
  const cinta = over(t['lavado-2'], plano)
  const pista = over(t['lavado-3'], plano)
  const realce = over(t.realce, plano)
  const solid = (k) => rgb(t[k]).c
  const surfaces = { noche: floor, plano, cinta, pista, realce }
  const check = (fgName, fg, bgName, bg, min) => {
    checked++
    const r = ratio(fg, bg)
    if (r < min) {
      bad++
      console.error(`FAIL ${name}: ${fgName} on ${bgName} = ${r.toFixed(2)}:1 (needs ${min}:1)`)
    }
  }
  // the two inks on every plane the interface paints
  for (const [sn, s] of Object.entries(surfaces)) {
    check('nieve', solid('nieve'), sn, s, 4.5)
    check('nieve-2', solid('nieve-2'), sn, s, 4.5)
    check('marca-clara', solid('marca-clara'), sn, s, 4.5)
  }
  // the slab and the primary action
  check('marca-tinta', solid('marca-tinta'), 'marca', solid('marca'), 4.5)
  check('marca-sobre', solid('marca-sobre'), 'marca', solid('marca'), 4.5)
  check('marca-tinta', solid('marca-tinta'), 'marca-honda', solid('marca-honda'), 4.5)
  check('marca-clara', solid('marca-clara'), 'marca-lavado', solid('marca-lavado'), 4.5)
  // the inverse button (white on dark, ink on light)
  check('tinta-inversa', solid('tinta-inversa'), 'fondo-inverso', solid('fondo-inverso'), 4.5)
  // status families: ink on its own field, and the field's ink on the floor it sits on
  for (const k of ['ok', 'rev', 'fdr']) {
    check(`${k}-tinta`, solid(`${k}-tinta`), `${k}-campo`, solid(`${k}-campo`), 4.5)
    check(`${k}-tinta`, solid(`${k}-tinta`), 'noche', floor, 4.5)
  }
  check('nau-tinta', solid('nau-tinta'), 'nau-campo', solid('nau-campo'), 4.5)
  // the border that says where a control starts (WCAG 1.4.11)
  check('borde-control', solid('borde-control'), 'noche', floor, 3)
  check('borde-control', solid('borde-control'), 'plano', plano, 3)
}
if (bad) process.exit(1)
console.log(`contrast OK: ${checked} checks across both themes`)
