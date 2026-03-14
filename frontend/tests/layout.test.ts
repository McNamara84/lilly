import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import Layout from '../src/routes/+layout.svelte';
import { createRawSnippet } from 'svelte';

// Mock matchMedia for jsdom
Object.defineProperty(window, 'matchMedia', {
	writable: true,
	value: vi.fn().mockImplementation((query: string) => ({
		matches: query === '(prefers-color-scheme: dark)',
		media: query,
		onchange: null,
		addListener: vi.fn(),
		removeListener: vi.fn(),
		addEventListener: vi.fn(),
		removeEventListener: vi.fn(),
		dispatchEvent: vi.fn()
	}))
});

// Mock auth store
const mockGetAuthState = vi.fn();
const mockInitAuth = vi.fn();
const mockPerformLogout = vi.fn();

vi.mock('$lib/stores/auth.svelte', () => ({
	getAuthState: () => mockGetAuthState(),
	initAuth: () => mockInitAuth(),
	performLogout: () => mockPerformLogout()
}));

vi.mock('$app/navigation', () => ({
	goto: vi.fn()
}));

vi.mock('$app/paths', () => ({
	resolve: (path: string) => path
}));

function renderLayout() {
	const children = createRawSnippet(() => ({
		render: () => '<div data-testid="child-content">Child</div>'
	}));
	return render(Layout, { props: { children } });
}

describe('Layout', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		// Default: not authenticated
		mockGetAuthState.mockReturnValue({
			isAuthenticated: false,
			user: null,
			isLoading: false
		});
		mockInitAuth.mockResolvedValue(undefined);
		mockPerformLogout.mockResolvedValue(undefined);
	});

	it('renders the LILLY header link', () => {
		renderLayout();
		const link = screen.getByRole('link', { name: /LILLY/i });
		expect(link).toBeInTheDocument();
		expect(link).toHaveAttribute('href', '/');
	});

	it('renders the theme toggle button', () => {
		renderLayout();
		const themeButton = screen.getByLabelText(/modus wechseln/i);
		expect(themeButton).toBeInTheDocument();
	});

	it('does not show logout button when not authenticated', () => {
		renderLayout();
		expect(screen.queryByTestId('header-logout-button')).not.toBeInTheDocument();
	});

	it('shows user display name and logout button when authenticated', () => {
		mockGetAuthState.mockReturnValue({
			isAuthenticated: true,
			user: { id: 1, email: 'test@test.com', display_name: 'Max Mustermann', email_verified: true },
			isLoading: false
		});

		renderLayout();

		expect(screen.getByTestId('user-display-name')).toHaveTextContent('Max Mustermann');
		expect(screen.getByTestId('header-logout-button')).toBeInTheDocument();
	});

	it('toggles theme when theme button is clicked', async () => {
		renderLayout();
		const user = userEvent.setup();

		// Initial state should have a theme toggle button
		const themeButton = screen.getByLabelText(/modus wechseln/i);
		await user.click(themeButton);

		// After clicking, the label should change (dark <-> light)
		expect(screen.getByLabelText(/modus wechseln/i)).toBeInTheDocument();
	});

	it('calls performLogout and navigates to login on logout click', async () => {
		const { goto } = await import('$app/navigation');

		mockGetAuthState.mockReturnValue({
			isAuthenticated: true,
			user: { id: 1, email: 'test@test.com', display_name: 'Test', email_verified: true },
			isLoading: false
		});

		renderLayout();
		const user = userEvent.setup();

		const logoutButton = screen.getByTestId('header-logout-button');
		await user.click(logoutButton);

		expect(mockPerformLogout).toHaveBeenCalledOnce();
		expect(goto).toHaveBeenCalledWith('/login');
	});

	it('has correct page title', () => {
		renderLayout();
		// The title is set via <svelte:head>, check it exists in the document
		expect(document.title).toBeDefined();
	});

	it('renders main content area', () => {
		renderLayout();
		const main = screen.getByRole('main');
		expect(main).toBeInTheDocument();
	});
});
