/* Owner controls for a headless frame. No device-local input is needed. */
class MemoryLaneUpdates {
  constructor(container) {
    this.container = container;
    this.frame = null;
    this.data = null;
    this.busy = false;
    this.resetting = false;
    this.generation = 0;
    this.fetchId = 0;
    this.timer = null;
    this.homeBadge = document.querySelector('#home-update-badge');
    this.settingsButton = document.querySelector('#open-device-settings');
    const heading = document.createElement('h2');
    heading.textContent = 'Software update';
    this.detail = document.createElement('p');
    this.detail.className = 'instrument-copy';
    this.progress = document.createElement('p');
    this.progress.className = 'instrument-copy';
    this.progress.setAttribute('role', 'status');
    this.progress.setAttribute('aria-live', 'polite');
    const actions = document.createElement('div');
    actions.className = 'field-row update-actions';
    this.buttons = {};
    for (const [action, label] of [['update', 'Install update'], ['check', 'Check for updates'], ['rollback', 'Restore previous version']]) {
      const button = document.createElement('button');
      button.type = 'button';
      button.className = action === 'update' ? 'signal-button update-install' : 'quiet-button';
      button.textContent = label;
      button.addEventListener('click', () => this.request(action));
      this.buttons[action] = button;
      actions.append(button);
    }
    container.replaceChildren(heading, this.detail, this.progress, actions);
    window.addEventListener('hashchange', () => {
      if (this.shouldPoll()) this.refresh();
      else clearTimeout(this.timer);
    });
    document.addEventListener('visibilitychange', () => {
      if (this.shouldPoll()) this.refresh();
      else clearTimeout(this.timer);
    });
  }
  shouldPoll() {
    return !document.hidden && ['', '#home', '#connected-panel', '#device-settings'].includes(location.hash);
  }
  setFrame(frame, resetting) {
    this.resetting = resetting;
    if (this.frame?.frame_id === frame?.frame_id) { this.paint(); return; }
    this.frame = frame || null;
    this.data = null;
    this.generation++;
    this.busy = false;
    clearTimeout(this.timer);
    this.paint();
    if (frame) this.refresh();
  }
  endpoint() { return `/api/beta/owner/frames/${encodeURIComponent(this.frame.frame_id)}/updates`; }
  async refresh() {
    clearTimeout(this.timer);
    if (!this.frame || this.busy) return;
    const generation = this.generation;
    const fetchId = ++this.fetchId;
    try {
      const response = await fetch(this.endpoint(), {cache: 'no-store'});
      if (!response.ok) throw new Error('Could not check update status. Try again shortly.');
      const data = await response.json();
      if (generation !== this.generation || fetchId !== this.fetchId) return;
      this.data = data;
      this.paint();
    } catch (error) {
      if (generation !== this.generation || fetchId !== this.fetchId) return;
      this.progress.textContent = error.message;
    } finally {
      if (generation === this.generation && fetchId === this.fetchId && this.shouldPoll()) {
        this.timer = setTimeout(() => this.refresh(), location.hash === '#device-settings' ? 5000 : 30000);
      }
    }
  }
  paint() {
    this.container.hidden = !this.frame;
    const report = this.data?.report || {};
    const command = this.data?.command;
    const supported = Boolean(report.updater_version);
    const active = ['pending', 'running'].includes(command?.state);
    const current = this.version(report.installed_release);
    const fresh = report.metadata_expires_unix > Date.now() / 1000;
    const available = fresh && report.available_release && report.available_release !== report.installed_release;
    if (this.homeBadge) {
      this.homeBadge.hidden = !available;
    }
    if (this.settingsButton) {
      const label = available ? `Settings — software update ${report.available_version || ''} available` : 'Settings';
      this.settingsButton.setAttribute('aria-label', label);
      this.settingsButton.title = label;
    }
    this.detail.textContent = !this.data ? 'Checking the frame’s software…'
      : !supported ? 'Remote software updates are not available on this frame yet.'
      : `${current ? `Installed: ${current}. ` : 'Current installation. '}${available ? `Version ${report.available_version} is available.` : fresh ? 'Your frame is up to date.' : 'Check for updates to see the latest version.'}`;
    if (this.resetting) this.progress.textContent = 'Finish resetting this frame before updating its software.';
    else if (active) this.progress.textContent = command.state === 'pending' ? 'Request received. Waiting for the frame to connect…'
      : command.action === 'check' ? 'The frame is checking for updates…'
      : command.action === 'rollback' ? 'Restoring the previous version. Photos may pause briefly…'
      : 'Installing the update. Photos may pause briefly…';
    else if (command?.state === 'failed') this.progress.textContent = command.message || 'The request could not be completed. Check the frame and try again.';
    else if (command?.state === 'succeeded') this.progress.textContent = command.message || 'Request complete.';
    else if (supported && this.data.reported_unix < Date.now() / 1000 - 90) this.progress.textContent = 'The frame has not connected recently. Requests wait up to 30 minutes for it to reconnect.';
    else this.progress.textContent = supported ? 'Checks run automatically. Updates install only when you choose.' : '';
    for (const button of Object.values(this.buttons)) button.disabled = !supported || active || this.busy || this.resetting;
    this.buttons.update.hidden = !available;
    this.buttons.rollback.hidden = !report.previous_release;
  }
  version(id) { return id && id !== 'baseline' ? id.slice(id.indexOf('-') + 1) : ''; }
  async request(action) {
    if (!this.frame || this.busy) return;
    const report = this.data?.report || {};
    if (action === 'update' && !confirm(`Install Memory Lane ${report.available_version} on “${this.frame.place_name}”? Photos will pause briefly while the frame updates. Wi-Fi settings and saved photos will be kept.`)) return;
    if (action === 'rollback' && !confirm(`Restore the previous software version on “${this.frame.place_name}”? Photos will pause briefly. Wi-Fi settings and saved photos will be kept.`)) return;
    const generation = this.generation;
    this.busy = true;
    this.fetchId++;
    clearTimeout(this.timer);
    this.paint();
    this.progress.textContent = 'Sending request to the frame…';
    try {
      const response = await fetch(this.endpoint(), {method: 'POST', headers: {'Content-Type': 'application/json'},
        body: JSON.stringify({action, release_id: action === 'update' ? report.available_release : action === 'rollback' ? report.previous_release : null})});
      if (!response.ok) throw new Error((await response.text()).slice(0, 300) || 'Could not send this request. Try again.');
      const data = await response.json();
      if (generation !== this.generation) return;
      this.data = data;
    } catch (error) {
      if (generation !== this.generation) return;
      this.progress.textContent = error.message;
      this.busy = false;
      for (const button of Object.values(this.buttons)) button.disabled = this.resetting;
      this.timer = setTimeout(() => this.refresh(), 5000);
      return;
    } finally {
      if (generation === this.generation) this.busy = false;
    }
    if (generation === this.generation) { this.paint(); this.refresh(); }
  }
}
