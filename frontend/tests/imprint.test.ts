import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import { readImprintConfig, type ImprintConfig } from '../src/lib/imprint';
import ImprintPage from '../src/routes/imprint/+page.svelte';

vi.mock('$app/paths', () => ({
	resolve: (path: string) => path
}));

const completeEnv = {
	IMPRINT_OPERATOR_NAME: 'Beispiel Betreiber e.V.',
	IMPRINT_ADDRESS: 'Musterstraße 1 | 12345 Musterstadt | Deutschland',
	IMPRINT_EMAIL: 'kontakt@example.org',
	IMPRINT_PHONE: '+49 123 456789',
	IMPRINT_RESPONSIBLE_PERSON: 'Erika Mustermann',
	IMPRINT_ADDITIONAL: 'Vereinsregister: VR 1234 | USt-IdNr.: DE123456789',
	IMPRINT_LAST_UPDATED: '2026-09-20'
};

describe('readImprintConfig', () => {
	it('reads the operator details from IMPRINT_* variables', () => {
		expect(readImprintConfig(completeEnv)).toEqual({
			configured: true,
			operatorName: 'Beispiel Betreiber e.V.',
			addressLines: ['Musterstraße 1', '12345 Musterstadt', 'Deutschland'],
			email: 'kontakt@example.org',
			phone: '+49 123 456789',
			responsiblePerson: 'Erika Mustermann',
			additionalParagraphs: ['Vereinsregister: VR 1234', 'USt-IdNr.: DE123456789'],
			lastUpdated: '2026-09-20'
		});
	});

	it('is not configured until name, address and email are all present', () => {
		expect(readImprintConfig({}).configured).toBe(false);
		expect(readImprintConfig({ ...completeEnv, IMPRINT_EMAIL: '  ' }).configured).toBe(false);
		expect(readImprintConfig({ ...completeEnv, IMPRINT_ADDRESS: ' | ' }).configured).toBe(false);
		expect(readImprintConfig({ ...completeEnv, IMPRINT_OPERATOR_NAME: undefined }).configured).toBe(
			false
		);
	});

	it('bounds the number and length of configured lines', () => {
		const config = readImprintConfig({
			...completeEnv,
			IMPRINT_ADDRESS: Array.from({ length: 20 }, (_, i) => `Zeile ${i}`).join('|'),
			IMPRINT_PHONE: '1'.repeat(200)
		});

		expect(config.addressLines).toHaveLength(6);
		expect(config.phone).toHaveLength(64);
	});
});

describe('Imprint page', () => {
	function renderPage(imprint: ImprintConfig) {
		return render(ImprintPage, { props: { data: { imprint } } });
	}

	it('renders the configured operator details', () => {
		renderPage(readImprintConfig(completeEnv));

		expect(screen.getByRole('heading', { name: 'Impressum' })).toBeInTheDocument();
		expect(screen.getByTestId('imprint-operator')).toHaveTextContent('Beispiel Betreiber e.V.');
		expect(screen.getByTestId('imprint-operator')).toHaveTextContent('12345 Musterstadt');
		expect(screen.getByRole('link', { name: 'kontakt@example.org' })).toHaveAttribute(
			'href',
			'mailto:kontakt@example.org'
		);
		expect(screen.getByTestId('imprint-responsible')).toHaveTextContent('Erika Mustermann');
		expect(screen.getByText('Vereinsregister: VR 1234')).toBeInTheDocument();
		expect(screen.getByTestId('imprint-last-updated')).toHaveTextContent('Stand: 2026-09-20');
		expect(screen.queryByTestId('imprint-unconfigured')).not.toBeInTheDocument();
	});

	it('shows how to configure the imprint when the operator has not provided one', () => {
		renderPage(readImprintConfig({}));

		expect(screen.getByTestId('imprint-unconfigured')).toHaveTextContent('IMPRINT_OPERATOR_NAME');
		expect(screen.queryByTestId('imprint-operator')).not.toBeInTheDocument();
		expect(screen.getByRole('link', { name: 'Datenschutzerklärung' })).toHaveAttribute(
			'href',
			'/privacy'
		);
	});

	it('renders configured values as text, never as markup', () => {
		renderPage(
			readImprintConfig({ ...completeEnv, IMPRINT_OPERATOR_NAME: '<img src=x onerror=alert(1)>' })
		);

		expect(screen.getByTestId('imprint-operator').querySelector('img')).toBeNull();
		expect(screen.getByTestId('imprint-operator')).toHaveTextContent(
			'<img src=x onerror=alert(1)>'
		);
	});
});
