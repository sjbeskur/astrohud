/* Playback order is separate from decoding, prefetching and network refreshes. */
class MemoryLanePlaybackOrder {
  constructor(random = Math.random, storage = undefined) {
    this.random = random;
    try { this.storage = storage === undefined ? globalThis.localStorage : storage; } catch { this.storage = null; }
    this.frame = null; this.mode = 'fresh_prints'; this.ids = []; this.remaining = []; this.seen = new Set(); this.current = null; this.pending = null; this.ownerRevision = 0;
  }
  reconcile(frame, mode, ids) {
    if (!['scenic_route', 'fresh_prints', 'serendipity'].includes(mode)) mode = 'fresh_prints';
    if (frame !== this.frame) {
      this.frame = frame; this.mode = mode; this.remaining = []; this.seen.clear(); this.current = null; this.pending = null; this.ownerRevision = 0;
      try {
        const raw = this.storage?.getItem(`memory-lane-playback-order:${frame}`);
        if (raw && raw.length <= 2 * 1024 * 1024) {
          const saved = JSON.parse(raw);
          if (saved.version === 1 && Number.isSafeInteger(saved.ownerRevision)) this.ownerRevision = saved.ownerRevision;
          const valid = list => Array.isArray(list) && list.length <= 2000 && list.every(id => typeof id === 'string' && id.length <= 200);
          if (saved.version === 1 && saved.mode === mode && valid(saved.remaining) && valid(saved.seen)) {
            this.remaining = saved.remaining; this.seen = new Set(saved.seen);
            this.current = typeof saved.current === 'string' ? saved.current : null;
          }
        }
      } catch { /* Progress is best effort when browser storage is unavailable. */ }
    } else if (this.mode !== mode) {
      this.mode = mode; this.remaining = []; this.seen.clear(); this.current = null; this.pending = null;
    }
    this.ids = [...new Set(ids)];
    const available = new Set(this.ids);
    this.remaining = this.remaining.filter(id => available.has(id));
    this.seen = new Set([...this.seen].filter(id => available.has(id)));
    if (!available.has(this.current)) this.current = null;
    if (!available.has(this.pending)) this.pending = null;
    const queued = new Set(this.remaining);
    for (const id of this.ids) if (!this.seen.has(id) && !queued.has(id)) { queued.add(id); this.remaining.push(id); }
  }
  peek() {
    if (!this.ids.length) return null;
    if (this.pending !== null) return this.pending;
    if (this.mode === 'fresh_prints') this.pending = this.ids[(this.ids.indexOf(this.current) + 1) % this.ids.length];
    else if (this.mode === 'serendipity') this.pending = this.ids[Math.floor(this.random() * this.ids.length)];
    else {
      if (!this.remaining.length) { this.seen.clear(); this.remaining = [...this.ids]; }
      for (let i = this.remaining.length - 1; i > 0; i--) {
        const j = Math.floor(this.random() * (i + 1));
        [this.remaining[i], this.remaining[j]] = [this.remaining[j], this.remaining[i]];
      }
      const last = this.remaining.length - 1;
      if (last > 0 && this.remaining[last] === this.current) {
        const other = Math.floor(this.random() * last);
        [this.remaining[last], this.remaining[other]] = [this.remaining[other], this.remaining[last]];
      }
      this.pending = this.remaining[last];
    }
    return this.pending;
  }
  consumeOwnerRevision(revision) { this.ownerRevision = revision; this.save(); }
  shown(id) {
    if (!this.ids.includes(id)) return;
    this.current = id; this.pending = null;
    if (this.mode === 'scenic_route') { this.seen.add(id); this.remaining = this.remaining.filter(p => p !== id); }
    this.save();
  }
  save() {
    try {
      this.storage?.setItem(`memory-lane-playback-order:${this.frame}`, JSON.stringify({version: 1, mode: this.mode, remaining: this.remaining, seen: [...this.seen], current: this.current, ownerRevision: this.ownerRevision}));
    } catch { /* Playback still works without persistent browser storage. */ }
  }
}

/* Shared browser display: one visible photo and a bounded next-photo cache. */
class MemoryLaneSlideshow {
  constructor(image, backdrop, empty) {
    this.image = image; this.backdrop = backdrop; this.empty = empty;
    this.photos = []; this.playback = {}; this.cache = new Map();
    this.order = new MemoryLanePlaybackOrder();
    this.currentId = undefined; this.seenRevision = 0; this.nextAt = 0; this.retryAt = 0; this.generation = 0;
    this.seenNextRevision = 0;
    this.timer = setInterval(() => this.tick().catch(() => {}), 250);
  }
  async update(manifest, headers = {}) {
    if (this.frameId !== manifest.frame_id) {
      this.frameId = manifest.frame_id;
      try { this.seenNextRevision = Number(localStorage.getItem(`memory-lane-guest-display:${this.frameId}`)) || 0; }
      catch { this.seenNextRevision = 0; }
    }
    this.generation++; this.headers = headers;
    this.photos = [...manifest.photos].sort((a,b) => (b.created_at || '').localeCompare(a.created_at || '') || (b.upload_order || 0) - (a.upload_order || 0)); const previous = this.playback.slide_seconds;
    const available = new Set(this.photos.map(photo => photo.id));
    for (const [id, promise] of this.cache) if (!available.has(id)) {
      this.cache.delete(id); promise.then(photo => URL.revokeObjectURL(photo.url)).catch(() => {});
    }
    this.playback = manifest.playback || {};
    this.order.reconcile(this.frameId, this.playback.playback_mode || 'fresh_prints', this.photos.map(p => p.id));
    this.seenRevision = this.order.ownerRevision;
    if (previous !== this.playback.slide_seconds) this.nextAt = Date.now() + this.interval();
    if (!this.photos.some(photo => photo.id === this.currentId)) {
      this.currentId = undefined; this.image.hidden = true; this.image.removeAttribute('src');
      this.backdrop.style.backgroundImage = ''; this.empty.hidden = false;
    }
    this.backdrop.hidden = this.playback.blur_background === false;
    await this.tick();
    const requested = this.requestedNext();
    if (requested && !this.pinned()) this.load(requested).catch(() => {});
  }
  interval() { return Math.max(5, Math.min(3600, this.playback.slide_seconds || 12)) * 1000; }
  pinned() {
    return (!this.playback.pinned_until || this.playback.pinned_until * 1000 > Date.now())
      ? this.photos.find(photo => photo.id === this.playback.pinned_photo_id) : undefined;
  }
  requestedNext() {
    return (this.playback.next_revision || 0) > this.seenNextRevision && this.playback.next_expires_at * 1000 > Date.now()
      ? this.photos.find(photo => photo.id === this.playback.next_photo_id) : undefined;
  }
  consumeNext() {
    this.seenNextRevision = this.playback.next_revision;
    try { localStorage.setItem(`memory-lane-guest-display:${this.frameId}`, String(this.seenNextRevision)); } catch {}
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
    let requested;
    if (changed && !this.playback.selected_photo_id) { this.seenRevision = revision; this.order.consumeOwnerRevision(revision); this.nextAt = 0; }
    let target = pinned || selected;
    if (!target) {
      if (this.currentId && Date.now() < this.nextAt) return;
      requested = this.requestedNext();
      target = requested || this.photos.find(photo => photo.id === this.order.peek());
    }
    if (!target) return;
    if (target.id === this.currentId) {
      if (requested) { this.consumeNext(); this.nextAt = Date.now() + this.interval(); }
      if (changed) { this.seenRevision = revision; this.order.consumeOwnerRevision(revision); this.nextAt = Date.now() + this.interval(); }
      if (!pinned && Date.now() >= this.nextAt) { this.order.shown(target.id); this.nextAt = Date.now() + this.interval(); }
      return;
    }
    this.busy = true; const generation = this.generation;
    try {
      const prepared = await this.load(target);
      if (generation !== this.generation) return;
      this.image.src = prepared.url; this.image.hidden = false; this.image.classList.add('ready');
      this.backdrop.style.backgroundImage = `url("${prepared.url}")`; this.empty.hidden = true;
      this.currentId = target.id; this.order.shown(target.id); this.nextAt = Date.now() + this.interval();
      if (requested) this.consumeNext();
      if (changed && (selected || !this.playback.selected_photo_id)) { this.seenRevision = revision; this.order.consumeOwnerRevision(revision); }
      const next = this.requestedNext() || this.photos.find(photo => photo.id === this.order.peek());
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
