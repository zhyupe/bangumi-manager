import { expect, test } from 'vitest'
import { cueTime } from './time'

test('formats track boundaries and durations without invalid second rollover', () => {
  expect(cueTime(0)).toBe('00:00')
  expect(cueTime(299.227)).toBe('04:59')
  expect(cueTime(59.999)).toBe('01:00')
  expect(cueTime(3661)).toBe('1:01:01')
  expect(cueTime(Number.NaN)).toBe('—')
})
