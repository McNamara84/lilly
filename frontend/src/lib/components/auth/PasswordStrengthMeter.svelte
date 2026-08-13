<script lang="ts">
	import type { PasswordStrengthResult } from '$lib/utils/password-strength';

	let {
		strength,
		id = 'password-strength-info'
	}: { strength: PasswordStrengthResult; id?: string } = $props();
</script>

<div class="mt-2" data-testid="password-strength">
	<div class="flex items-center justify-between mb-1">
		<span class="text-xs" style="color: var(--text-tertiary);">Passwortstärke</span>
		<span class="text-xs font-medium" style="color: {strength.color};">
			{strength.label}
		</span>
	</div>
	<div
		class="h-1.5 w-full rounded-full overflow-hidden"
		style="background-color: var(--surface-raised);"
		role="progressbar"
		aria-valuenow={strength.score}
		aria-valuemin={0}
		aria-valuemax={4}
		aria-label="Passwortstärke: {strength.label}"
		{id}
	>
		<div
			class="h-full rounded-full transition-all duration-300"
			style="width: {(strength.score + 1) * 20}%; background-color: {strength.color};"
		></div>
	</div>
</div>
