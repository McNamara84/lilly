import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import PrivacyPage from '../src/routes/privacy/+page.svelte';

vi.mock('$app/paths', () => ({
	resolve: (path: string) => path
}));

describe('Privacy Page', () => {
	it('renders the page heading', () => {
		render(PrivacyPage);

		expect(screen.getByRole('heading', { name: /Datenschutzerklärung/i })).toBeInTheDocument();
	});

	it('renders all data protection sections', () => {
		render(PrivacyPage);

		expect(screen.getByText(/1\. Verantwortlicher/)).toBeInTheDocument();
		expect(screen.getByText(/2\. Erhobene Daten/)).toBeInTheDocument();
		expect(screen.getByText(/3\. Zweck der Verarbeitung/)).toBeInTheDocument();
		expect(screen.getByText(/4\. Rechtsgrundlage/)).toBeInTheDocument();
		expect(screen.getByText(/5\. Speicherdauer/)).toBeInTheDocument();
		expect(screen.getByText(/6\. Ihre Rechte/)).toBeInTheDocument();
		expect(screen.getByText(/7\. Cookies/)).toBeInTheDocument();
		expect(screen.getByText(/8\. Kontakt/)).toBeInTheDocument();
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
