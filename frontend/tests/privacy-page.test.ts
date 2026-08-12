import { beforeEach, describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import PrivacyPage from '../src/routes/privacy/+page.svelte';

vi.mock('$app/paths', () => ({
	resolve: (path: string) => path
}));

vi.mock('$lib/api/auth', () => ({
	fetchAuthOptions: vi.fn()
}));

describe('Privacy Page', () => {
	beforeEach(async () => {
		vi.clearAllMocks();
		const { fetchAuthOptions } = await import('$lib/api/auth');
		vi.mocked(fetchAuthOptions).mockResolvedValue({
			privacy_policy: { version: 'test-v1', url: '/privacy' },
			oauth: { google: true, github: true }
		});
	});

	it('renders the current policy version from the public auth options', async () => {
		render(PrivacyPage);

		expect(await screen.findByTestId('privacy-policy-version')).toHaveTextContent(
			'Version test-v1'
		);
	});

	it('falls back to a loading label when auth options cannot be fetched', async () => {
		const { fetchAuthOptions } = await import('$lib/api/auth');
		vi.mocked(fetchAuthOptions).mockRejectedValue(new Error('Network failure'));

		render(PrivacyPage);

		expect(await screen.findByTestId('privacy-policy-version')).toHaveTextContent(
			'Version wird geladen'
		);
	});

	it('renders the page heading', () => {
		render(PrivacyPage);

		expect(screen.getByRole('heading', { name: /Datenschutzerklärung/i })).toBeInTheDocument();
	});

	it('renders all data protection sections', () => {
		render(PrivacyPage);

		expect(screen.getByText(/1\. Verantwortlicher/)).toBeInTheDocument();
		expect(screen.getByText(/2\. Erhobene Daten/)).toBeInTheDocument();
		expect(screen.getByText(/3\. Zweck der Verarbeitung/)).toBeInTheDocument();
		expect(screen.getByText(/4\. Anmeldung über Google oder GitHub/)).toBeInTheDocument();
		expect(screen.getByText(/5\. Öffentliche Freigaben/)).toBeInTheDocument();
		expect(screen.getByText(/6\. Rechtsgrundlage/)).toBeInTheDocument();
		expect(screen.getByText(/7\. Speicherdauer/)).toBeInTheDocument();
		expect(screen.getByText(/8\. Ihre Rechte/)).toBeInTheDocument();
		expect(screen.getByText(/9\. Cookies/)).toBeInTheDocument();
		expect(screen.getByText(/10\. Kontakt/)).toBeInTheDocument();
	});

	it('explains that public collections also publish personal issue notes', () => {
		render(PrivacyPage);

		expect(screen.getByText(/standardmäßig privat/)).toBeInTheDocument();
		expect(screen.getByText(/persönlichen Heftnotizen öffentlich angezeigt/)).toBeInTheDocument();
	});

	it('mentions Argon2id password hashing', () => {
		render(PrivacyPage);

		expect(screen.getByText(/Argon2id/)).toBeInTheDocument();
	});

	it('mentions HttpOnly cookies', () => {
		render(PrivacyPage);

		expect(screen.getByText(/HttpOnly/)).toBeInTheDocument();
	});

	it('has a back link to registration', () => {
		render(PrivacyPage);

		const link = screen.getByRole('link', { name: /Zurück zur Registrierung/i });
		expect(link).toBeInTheDocument();
		expect(link).toHaveAttribute('href', '/register');
	});

	it('lists user rights', () => {
		render(PrivacyPage);

		expect(screen.getByText(/Auskunft/)).toBeInTheDocument();
		expect(screen.getByText(/Berichtigung/)).toBeInTheDocument();
		expect(screen.getByText(/Löschung Ihrer Daten/)).toBeInTheDocument();
		expect(screen.getByText(/Datenübertragbarkeit/)).toBeInTheDocument();
	});
});
