/* Device storage belongs to the selected frame, including when it is offline. */
class MemoryLaneStorage {
  constructor(container, notice) {
    this.container = container;
    this.notice = notice;
    this.frame = null;
    this.data = null;
    this.generation = 0;
    this.fetchId = 0;
    this.timer = null;
    this.failed = false;
    const heading = document.createElement('h2');
    heading.textContent = 'Storage';
    this.status = document.createElement('p');
    this.status.className = 'storage-status';
    this.meter = document.createElement('progress');
    this.meter.className = 'storage-meter';
    this.meter.max = 100;
    this.meter.setAttribute('aria-label', 'Device storage used');
    this.capacity = document.createElement('p');
    this.capacity.className = 'storage-capacity';
    this.disk = document.createElement('p');
    this.disk.className = 'instrument-copy';
    this.photos = document.createElement('p');
    this.photos.className = 'instrument-copy';
    this.detail = document.createElement('p');
    this.detail.className = 'instrument-copy';
    this.updated = document.createElement('p');
    this.updated.className = 'instrument-copy storage-updated';
    container.replaceChildren(heading, this.status, this.meter, this.disk, this.capacity, this.photos, this.detail, this.updated);
    for (const [target,event] of [[window,'hashchange'],[document,'visibilitychange']]) {
      target.addEventListener(event, () => {
        clearTimeout(this.timer);
        if (this.shouldPoll()) this.refresh();
      });
    }
  }
  shouldPoll() {
    return !document.hidden && ['', '#home', '#connected-panel', '#device-settings'].includes(location.hash);
  }
  setFrame(frame) {
    if (this.frame?.frame_id === frame?.frame_id) { this.paint(); return; }
    this.frame = frame || null;
    this.data = null;
    this.failed = false;
    this.generation++;
    clearTimeout(this.timer);
    this.paint();
    if (frame) this.refresh();
  }
  async refresh() {
    clearTimeout(this.timer);
    if (!this.frame) return;
    const generation = this.generation;
    const fetchId = ++this.fetchId;
    try {
      const response = await fetch(`/api/beta/owner/frames/${encodeURIComponent(this.frame.frame_id)}/storage`, {cache:'no-store'});
      if (!response.ok) throw new Error('Storage unavailable');
      const data = await response.json();
      if (generation !== this.generation || fetchId !== this.fetchId) return;
      this.data = data;
      this.failed = false;
    } catch (_) {
      if (generation !== this.generation || fetchId !== this.fetchId) return;
      this.failed = true;
    } finally {
      if (generation === this.generation && fetchId === this.fetchId) {
        this.paint();
        if (this.shouldPoll()) this.timer = setTimeout(() => this.refresh(), 30000);
      }
    }
  }
  paint() {
    this.container.hidden = !this.frame;
    this.notice.hidden = true;
    const report = this.data?.report;
    const measured = Boolean(report);
    for (const element of [this.meter,this.disk,this.photos,this.detail,this.updated]) element.hidden = !measured;
    if (!measured) {
      this.status.textContent = this.failed ? 'Storage information is temporarily unavailable.' : this.data ? 'Waiting for storage information.' : 'Checking device storage…';
      this.capacity.textContent = this.data ? 'Your frame needs the latest viewer software and an internet connection to report its storage.' : '';
      this.container.dataset.status = 'unknown';
      return;
    }
    const age = Math.max(0, Date.now()/1000 - this.data.reported_unix);
    const stale = age > 600 || this.failed;
    const state = stale ? 'stale' : this.data.space_status;
    this.container.dataset.status = state;
    this.status.textContent = stale ? 'Last reported storage' : state === 'very_low' ? 'Very little space remaining' : state === 'low' ? 'Storage running low' : 'Storage healthy';
    this.meter.value = Math.min(100, Math.max(0, report.disk_used_bytes / report.disk_total_bytes * 100));
    this.meter.setAttribute('aria-valuetext', `${this.bytes(report.disk_used_bytes)} used of ${this.bytes(report.disk_total_bytes)}`);
    this.disk.textContent = `${this.bytes(report.disk_used_bytes)} used of ${this.bytes(report.disk_total_bytes)}`;
    const estimate = this.data.estimated_photos_remaining;
    this.capacity.textContent = estimate === null ? 'Add a few photos to estimate how many more will fit.'
      : estimate === 0 ? this.data.downloads_paused ? 'New photo downloads paused to protect device space.' : 'Room for fewer than one more typical photo within current limits.'
      : `${stale ? 'At last report, room' : 'Room'} for about ${this.number(estimate)} more ${estimate === 1 ? 'photo' : 'photos'}`;
    this.photos.textContent = `${this.number(report.photo_count)} ${report.photo_count === 1 ? 'photo' : 'photos'} on this device · ${this.bytes(report.photo_bytes)} of ${this.bytes(report.photo_limit_bytes)} photo storage`;
    this.detail.textContent = `${this.bytes(report.disk_available_bytes)} available on device. ` + (this.data.downloads_paused
      ? 'Existing photos keep playing. Downloads resume when space is available.'
      : 'The frame manages downloaded photos automatically. The estimate uses your photo sizes, current allowance, and space kept free for the device.');
    if (report.photo_count > 0 && report.photo_count < 5 && estimate > 0) this.detail.textContent += ' This early estimate may change as you add photos.';
    const minutes = Math.floor(age/60);
    const when = minutes < 1 ? 'just now' : minutes < 60 ? `${minutes} ${minutes === 1 ? 'minute' : 'minutes'} ago` : `${Math.floor(minutes/60)} ${minutes < 120 ? 'hour' : 'hours'} ago`;
    this.updated.textContent = stale ? `Last reported ${when}. ${this.failed ? 'Could not refresh this reading.' : 'The frame has not reported recently.'}` : `Updated ${when}`;
    if (!stale && ['low','very_low'].includes(state)) {
      this.notice.hidden = false;
      this.notice.textContent = state === 'very_low' ? 'Device storage very low' : 'Device storage low';
    }
  }
  number(value) { return Number(value).toLocaleString(); }
  bytes(value) {
    if (value < 1000000) return `${Math.round(value/1000)} KB`;
    if (value < 1000000000) return `${Math.round(value/1000000)} MB`;
    return `${(value/1000000000).toFixed(1)} GB`;
  }
}
