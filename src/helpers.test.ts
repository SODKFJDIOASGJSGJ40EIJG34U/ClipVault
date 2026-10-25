import { describe, expect, it } from 'vitest';
import { formatTime } from './helpers';

describe('formatTime', () => {
  it('formats valid timestamps for both supported locales', () => {
    expect(formatTime('2026-10-25T12:34:00.000Z', false)).not.toBe('');
    expect(formatTime('2026-10-25T12:34:00.000Z', true)).not.toBe('');
  });
  it('handles malformed persisted dates without throwing', () => {
    expect(formatTime('not-a-date', false)).toBe('');
  });
});
