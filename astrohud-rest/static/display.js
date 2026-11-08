/* Shared browser display: one visible photo and a bounded next-photo cache. */
class MemoryLaneSlideshow {
  constructor(image, backdrop, empty) {
    this.image = image; this.backdrop = backdrop; this.empty = empty;
    this.photos = []; this.playback = {}; this.cache = new Map();
    this.currentId = undefined; this.seenRevision = 0; this.nextAt = 0; this.retryAt = 0; this.generation = 0;
    this.timer = setInterval(() => this.tick().catch(() => {}), 250);
  }
  async update(manifest, headers = {}) {
    this.generation++; this.headers = headers;
    this.photos = manifest.photos; const previous = this.playback.slide_seconds;
    const available = new Set(this.photos.map(photo => photo.id));
    for (const [id, promise] of this.cache) if (!available.has(id)) {
      this.cache.delete(id); promise.then(photo => URL.revokeObjectURL(photo.url)).catch(() => {});
    }
    this.playback = manifest.playback || {};
    if (previous !== this.playback.slide_seconds) this.nextAt = Date.now() + this.interval();
    if (!this.photos.some(photo => photo.id === this.currentId)) {
      this.currentId = undefined; this.image.hidden = true; this.image.removeAttribute('src');
      this.backdrop.style.backgroundImage = ''; this.empty.hidden = false;
    }
    this.backdrop.hidden = this.playback.blur_background === false;
    await this.tick();
  }
  interval() { return Math.max(5, Math.min(3600, this.playback.slide_seconds || 12)) * 1000; }
  pinned() {
    return (!this.playback.pinned_until || this.playback.pinned_until * 1000 > Date.now())
      ? this.photos.find(photo => photo.id === this.playback.pinned_photo_id) : undefined;
  }
  load(photo) {
    if (this.cache.has(photo.id)) return this.cache.get(photo.id);
    const promise = (async () => {
      const url = new URL(photo.url, location.origin);
      if (url.origin !== location.origin) throw new Error('Unexpected photo origin');
      const response = await fetch(url, {headers: this.headers});
      if (!response.ok) throw new Error('Photo download failed');
      const blobUrl = URL.createObjectURL(await response.blob());
      const decoded = new Image(); decoded.src = blobUrl;
      try { await decoded.decode(); } catch (error) { URL.revokeObjectURL(blobUrl); throw error; }
      return {url: blobUrl, decoded};
    })().catch(error => { if (this.cache.get(photo.id) === promise) this.cache.delete(photo.id); throw error; });
    this.cache.set(photo.id, promise);
    return promise;
  }
  async tick() {
    if (this.busy || Date.now() < this.retryAt || !this.photos.length) return;
    const revision = this.playback.selection_revision || 0;
    const changed = revision !== this.seenRevision;
    const pinned = this.pinned();
    const selected = changed && this.photos.find(photo => photo.id === this.playback.selected_photo_id);
    if (changed && !this.playback.selected_photo_id) { this.seenRevision = revision; this.nextAt = 0; }
    let target = pinned || selected;
    if (!target) {
      if (this.currentId && Date.now() < this.nextAt) return;
      const index = this.photos.findIndex(photo => photo.id === this.currentId);
      target = this.photos[(index + 1) % this.photos.length];
    }
    if (target.id === this.currentId) {
      if (changed) { this.seenRevision = revision; this.nextAt = Date.now() + this.interval(); }
      return;
    }
    this.busy = true; const generation = this.generation;
    try {
      const prepared = await this.load(target);
      if (generation !== this.generation) return;
      this.image.src = prepared.url; this.image.hidden = false; this.image.classList.add('ready');
      this.backdrop.style.backgroundImage = `url("${prepared.url}")`; this.empty.hidden = true;
      this.currentId = target.id; this.nextAt = Date.now() + this.interval();
      if (changed && (selected || !this.playback.selected_photo_id)) this.seenRevision = revision;
      const index = this.photos.findIndex(photo => photo.id === this.currentId);
      const next = this.photos[(index + 1) % this.photos.length];
      const retain = new Set([this.currentId, next.id, pinned?.id]);
      for (const [id, promise] of this.cache) if (!retain.has(id)) {
        this.cache.delete(id); promise.then(photo => URL.revokeObjectURL(photo.url)).catch(() => {});
      }
      if (!pinned && next.id !== target.id) this.load(next).catch(() => {});
    } catch (error) { this.retryAt = Date.now() + 5000; throw error; }
    finally { this.busy = false; }
  }
  clear() {
    this.generation++; this.photos = []; this.currentId = undefined;
    this.image.hidden = true; this.image.removeAttribute('src'); this.backdrop.style.backgroundImage = '';
    this.empty.hidden = false;
    for (const promise of this.cache.values()) promise.then(photo => URL.revokeObjectURL(photo.url)).catch(() => {});
    this.cache.clear();
  }
}
