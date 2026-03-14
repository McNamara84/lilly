import { describe, it, expect } from 'vitest';
import { checkPasswordStrength, MIN_SCORE } from '../src/lib/utils/password-strength';

describe('Password Strength', () => {
	it('returns score 0 for empty password', () => {
		const result = checkPasswordStrength('');
		expect(result.score).toBe(0);
		expect(result.label).toBe('Sehr schwach');
	});

	it('returns low score for weak password', () => {
		const result = checkPasswordStrength('123456');
		expect(result.score).toBeLessThan(MIN_SCORE);
	});

	it('returns higher score for strong password', () => {
		const result = checkPasswordStrength('Kj$9mP!xL2@qWzR');
		expect(result.score).toBeGreaterThanOrEqual(MIN_SCORE);
	});

	it('penalizes password containing user inputs', () => {
		const withoutContext = checkPasswordStrength('maxmuster2024');
		const withContext = checkPasswordStrength('maxmuster2024', ['maxmuster']);
		expect(withContext.score).toBeLessThanOrEqual(withoutContext.score);
	});

	it('returns German labels', () => {
		const labels = ['Sehr schwach', 'Schwach', 'Ausreichend', 'Stark', 'Sehr stark'];
		const result = checkPasswordStrength('test');
		expect(labels).toContain(result.label);
	});

	it('returns color string for each score', () => {
		const result = checkPasswordStrength('test');
		expect(result.color).toMatch(/^#[0-9a-fA-F]{6}$/);
	});

	it('MIN_SCORE is 2', () => {
		expect(MIN_SCORE).toBe(2);
	});
});
