import assert from 'node:assert/strict'
import { test } from 'node:test'
import { ago, bytes, eta, initials, percent, speed } from './format.ts'

test('bytes are human readable and never NaN', () => {
  assert.equal(bytes(0), '0 B')
  assert.equal(bytes(-5), '0 B')
  assert.equal(bytes(NaN), '0 B')
  assert.equal(bytes(1023), '1023 B')
  assert.equal(bytes(1536), '1.5 KB')
  assert.equal(bytes(20_123_648), '19.2 MB')
  assert.equal(bytes(5 * 1024 ** 3), '5.0 GB')
})

test('speed and eta stay empty when unknown', () => {
  assert.equal(speed(0), '')
  assert.equal(speed(2048), '2.0 KB/s')
  assert.equal(eta(0, 100, 0), '')
  assert.equal(eta(100, 100, 50), '')
  assert.equal(eta(0, 1000, 100), '10 s')
  assert.equal(eta(0, 100_000, 1000), '1 min 40 s')
})

test('percent is clamped and safe on an unknown total', () => {
  assert.equal(percent(0, 0), 0)
  assert.equal(percent(50, 200), 25)
  assert.equal(percent(300, 200), 100)
})

test('initials', () => {
  assert.equal(initials('ASTERION BIM'), 'AB')
  assert.equal(initials('Hub'), 'HU')
  assert.equal(initials('  '), '?')
})

test('relative dates', () => {
  const now = Date.parse('2026-10-17T12:00:00Z')
  assert.equal(ago(null, now), '—')
  assert.equal(ago('garbage', now), '—')
  assert.equal(ago('2026-10-17T11:59:50Z', now), 'hace un momento')
  assert.match(ago('2026-10-17T11:00:00Z', now), /1 hora|hace 1 h/)
  assert.match(ago('2026-10-15T12:00:00Z', now), /anteayer|2 días/)
})
