<script lang="ts">
	import { onDestroy } from 'svelte';
	import {
		DEFAULT_PHOTO_POLICY,
		deleteCollectionPhoto,
		fetchCollectionPhotos,
		fetchPhotoPolicy,
		uploadCollectionPhoto,
		type CollectionPhoto,
		type PhotoPolicy
	} from '$lib/api/media';

	interface Props {
		entryId: number;
	}

	let { entryId }: Props = $props();
	let photos = $state<CollectionPhoto[]>([]);
	let policy = $state<PhotoPolicy>(DEFAULT_PHOTO_POLICY);
	let loading = $state(true);
	let uploading = $state(false);
	let deletingId = $state<number | null>(null);
	let progress = $state(0);
	let currentFileName = $state('');
	let pendingPreviewUrl = $state<string | null>(null);
	let message = $state<string | null>(null);
	let messageKind = $state<'status' | 'error'>('status');
	let selectedPhoto = $state<CollectionPhoto | null>(null);
	let fileInput = $state<HTMLInputElement>();
	let uploadController: AbortController | null = null;

	const remainingSlots = $derived(Math.max(0, policy.max_photos - photos.length));
	const acceptedTypes = $derived(policy.allowed_media_types.join(','));
	const maxSizeLabel = $derived(formatBytes(policy.max_upload_bytes));

	$effect(() => {
		const currentEntryId = entryId;
		const controller = new AbortController();
		loading = true;
		message = null;
		Promise.all([
			fetchPhotoPolicy(controller.signal).catch(() => DEFAULT_PHOTO_POLICY),
			fetchCollectionPhotos(currentEntryId, controller.signal)
		])
			.then(([loadedPolicy, loadedPhotos]) => {
				if (controller.signal.aborted) return;
				policy = loadedPolicy;
				photos = sortPhotos(loadedPhotos);
			})
			.catch((error: unknown) => {
				if (controller.signal.aborted) return;
				showMessage(errorMessage(error, 'Fotos konnten nicht geladen werden.'), 'error');
			})
			.finally(() => {
				if (!controller.signal.aborted) loading = false;
			});
		return () => controller.abort();
	});

	function openFilePicker() {
		if (!uploading && remainingSlots > 0) fileInput?.click();
	}

	async function handleFileSelection(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		await uploadFiles(input.files);
		input.value = '';
	}

	async function handleDrop(event: DragEvent) {
		event.preventDefault();
		if (uploading || remainingSlots === 0) return;
		await uploadFiles(event.dataTransfer?.files ?? null);
	}

	function allowDrop(event: DragEvent) {
		if (!uploading && remainingSlots > 0) event.preventDefault();
	}

	async function uploadFiles(files: FileList | null) {
		if (!files?.length || uploading) return;
		message = null;
		const candidates = Array.from(files).slice(0, remainingSlots);
		if (files.length > remainingSlots) {
			showMessage(`Es sind nur noch ${remainingSlots} Foto-Slots frei.`, 'error');
		}
		for (const file of candidates) {
			const validationError = validateFile(file);
			if (validationError) {
				showMessage(validationError, 'error');
				continue;
			}
			uploading = true;
			progress = 0;
			currentFileName = file.name;
			const previewUrl = URL.createObjectURL(file);
			pendingPreviewUrl = previewUrl;
			const controller = new AbortController();
			uploadController = controller;
			try {
				const photo = await uploadCollectionPhoto(
					entryId,
					file,
					(value) => (progress = value),
					controller.signal
				);
				photos = sortPhotos([...photos, photo]);
				showMessage(`„${file.name}“ wurde hochgeladen.`, 'status');
			} catch (error) {
				showMessage(errorMessage(error, 'Foto konnte nicht hochgeladen werden.'), 'error');
			} finally {
				URL.revokeObjectURL(previewUrl);
				if (pendingPreviewUrl === previewUrl) pendingPreviewUrl = null;
				if (uploadController === controller) uploadController = null;
				uploading = false;
				progress = 0;
				currentFileName = '';
			}
		}
	}

	function validateFile(file: File): string | null {
		if (!policy.allowed_media_types.includes(file.type)) {
			return `„${file.name}“ ist kein unterstütztes JPEG-, PNG- oder WebP-Bild.`;
		}
		if (file.size > policy.max_upload_bytes) {
			return `„${file.name}“ ist größer als ${maxSizeLabel}.`;
		}
		return null;
	}

	async function removePhoto(photo: CollectionPhoto) {
		if (deletingId !== null || !window.confirm('Dieses Foto wirklich löschen?')) return;
		deletingId = photo.id;
		message = null;
		try {
			await deleteCollectionPhoto(entryId, photo.id);
			photos = photos.filter((candidate) => candidate.id !== photo.id);
			if (selectedPhoto?.id === photo.id) selectedPhoto = null;
			showMessage('Foto wurde gelöscht.', 'status');
		} catch (error) {
			showMessage(errorMessage(error, 'Foto konnte nicht gelöscht werden.'), 'error');
		} finally {
			deletingId = null;
		}
	}

	function showMessage(text: string, kind: 'status' | 'error') {
		message = text;
		messageKind = kind;
	}

	function sortPhotos(items: CollectionPhoto[]) {
		return [...items].sort(
			(left, right) => left.sort_order - right.sort_order || left.id - right.id
		);
	}

	function formatBytes(bytes: number) {
		return `${Math.round((bytes / 1024 / 1024) * 10) / 10} MiB`;
	}

	function errorMessage(error: unknown, fallback: string) {
		return error instanceof Error && error.message ? error.message : fallback;
	}

	onDestroy(() => uploadController?.abort());
</script>

<section class="mt-5" aria-labelledby="photo-uploader-heading" data-testid="photo-uploader">
	<div class="mb-2 flex items-center justify-between gap-3">
		<h3
			id="photo-uploader-heading"
			class="text-sm font-semibold"
			style="color: var(--text-primary);"
		>
			Eigene Fotos
		</h3>
		<span class="text-xs" style="color: var(--text-tertiary);" data-testid="photo-count">
			{photos.length}/{policy.max_photos}
		</span>
	</div>

	{#if loading}
		<p class="text-xs" style="color: var(--text-tertiary);">Fotos werden geladen …</p>
	{:else}
		{#if photos.length > 0 || pendingPreviewUrl}
			<div class="mb-3 grid grid-cols-2 gap-3 sm:grid-cols-4" data-testid="photo-grid">
				{#each photos as photo (photo.id)}
					<article
						class="relative overflow-hidden rounded-lg"
						style="border: 1px solid var(--glass-border);"
					>
						<button
							type="button"
							class="block aspect-square w-full cursor-zoom-in"
							onclick={() => (selectedPhoto = photo)}
							aria-label="Foto {photo.sort_order + 1} vergrößern"
						>
							<img
								src={photo.content_url}
								alt="Eigenes Foto {photo.sort_order + 1} des Sammlungsexemplars"
								class="h-full w-full object-cover"
								loading="lazy"
							/>
						</button>
						<button
							type="button"
							class="absolute right-1 top-1 rounded-md px-2 py-1 text-xs font-semibold cursor-pointer"
							style="background: rgb(0 0 0 / 75%); color: white;"
							disabled={deletingId !== null}
							onclick={() => removePhoto(photo)}
							aria-label="Foto {photo.sort_order + 1} löschen"
							data-testid="delete-photo-{photo.id}"
						>
							{deletingId === photo.id ? '…' : 'Löschen'}
						</button>
					</article>
				{/each}
				{#if pendingPreviewUrl}
					<article
						class="relative aspect-square overflow-hidden rounded-lg opacity-75"
						style="border: 1px solid var(--glass-border);"
						data-testid="pending-photo-preview"
					>
						<img
							src={pendingPreviewUrl}
							alt="Lokale Vorschau von {currentFileName}"
							class="h-full w-full object-cover"
						/>
						<span
							class="absolute inset-x-0 bottom-0 p-1 text-center text-xs font-semibold"
							style="background: rgb(0 0 0 / 75%); color: white;"
						>
							Upload {progress}%
						</span>
					</article>
				{/if}
			</div>
		{/if}

		<input
			bind:this={fileInput}
			type="file"
			class="sr-only"
			accept={acceptedTypes}
			capture="environment"
			multiple
			onchange={handleFileSelection}
			disabled={uploading || remainingSlots === 0}
			aria-label="Fotos aufnehmen oder auswählen"
			data-testid="photo-file-input"
		/>

		<button
			type="button"
			class="w-full rounded-lg border-2 border-dashed p-4 text-center transition-colors"
			style="border-color: var(--glass-border); background: var(--glass); color: var(--text-secondary);"
			disabled={uploading || remainingSlots === 0}
			onclick={openFilePicker}
			ondragover={allowDrop}
			ondrop={handleDrop}
			data-testid="photo-dropzone"
		>
			{#if uploading}
				<span class="block text-sm font-medium">{currentFileName}: {progress}%</span>
				<span
					class="mt-2 block h-2 overflow-hidden rounded-full"
					style="background: var(--glass-border);"
					role="progressbar"
					aria-valuenow={progress}
					aria-valuemin="0"
					aria-valuemax="100"
				>
					<span
						class="block h-full transition-[width]"
						style={`width: ${progress}%; background: var(--color-brand-500);`}
					></span>
				</span>
			{:else if remainingSlots === 0}
				<span class="text-sm font-medium">Alle vier Foto-Slots sind belegt</span>
			{:else}
				<span class="block text-sm font-medium">Foto aufnehmen oder auswählen</span>
				<span class="mt-1 block text-xs">
					JPEG, PNG oder WebP bis {maxSizeLabel} · noch {remainingSlots}
					{remainingSlots === 1 ? 'Slot' : 'Slots'}
				</span>
			{/if}
		</button>
	{/if}

	{#if message}
		<p
			class="mt-2 text-xs"
			style={`color: ${messageKind === 'error' ? 'var(--color-error)' : 'var(--text-secondary)'};`}
			role={messageKind === 'error' ? 'alert' : 'status'}
			data-testid="photo-message"
		>
			{message}
		</p>
	{/if}
</section>

{#if selectedPhoto}
	<div
		class="fixed inset-0 z-[70] flex items-center justify-center bg-black/80 p-4"
		role="presentation"
	>
		<button
			type="button"
			class="absolute inset-0 cursor-zoom-out"
			onclick={() => (selectedPhoto = null)}
			aria-label="Vergrößerte Fotoansicht schließen"
		></button>
		<div
			class="relative max-h-full max-w-4xl"
			role="dialog"
			aria-modal="true"
			aria-label="Fotoansicht"
		>
			<img
				src={selectedPhoto.content_url}
				alt="Vergrößertes eigenes Foto des Sammlungsexemplars"
				class="max-h-[90vh] max-w-full rounded-lg object-contain"
			/>
		</div>
	</div>
{/if}
