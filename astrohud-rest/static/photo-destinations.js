/* Explicit send destinations, shared by the owner and invited-sender forms. */
class MemoryLaneDestinations {
  constructor(root, onChange = () => {}) {
    this.root = root; this.onChange = onChange; this.frames = []; this.mode = 'current'; this.chosen = new Set();
  }
  configure(frames, current) {
    if (current !== this.current) { this.mode = 'current'; this.chosen = new Set([current]); }
    this.frames = frames; this.current = current;
    this.chosen = new Set([...this.chosen].filter(id => frames.some(frame => frame.frame_id === id)));
    this.render(); this.onChange();
  }
  get recipients() {
    return this.frames.filter(frame => this.mode === 'all' || (this.mode === 'current' ? frame.frame_id === this.current : this.chosen.has(frame.frame_id)));
  }
  get selection() {
    return this.mode === 'all' ? {mode:'all'} : {mode:'frames', frame_ids:this.recipients.map(frame => frame.frame_id)};
  }
  get summary() {
    return this.mode === 'all' ? `All ${this.frames.length} frames` : this.recipients.map(frame => frame.place_name).join(' · ');
  }
  render() {
    this.root.hidden = this.frames.length < 2;
    this.root.replaceChildren();
    const legend = document.createElement('legend'); legend.textContent = 'Send to';
    const modes = document.createElement('div'); modes.className = 'destination-modes';
    modes.setAttribute('role','group'); modes.setAttribute('aria-label','Photo destinations');
    for (const [mode,label] of [['current','This frame'],['choose','Choose frames'],['all',`All ${this.frames.length} frames`]]) {
      const button = document.createElement('button'); button.type = 'button'; button.className = 'quiet-button'; button.textContent = label; button.dataset.destinationMode = mode;
      button.setAttribute('aria-pressed', String(this.mode === mode));
      button.addEventListener('click', () => {
        this.mode = mode; this.render(); this.root.querySelector(`[data-destination-mode="${mode}"]`).focus(); this.onChange();
      });
      modes.append(button);
    }
    this.root.append(legend,modes);
    if (this.mode === 'choose') {
      const choices = document.createElement('div'); choices.className = 'destination-choices';
      for (const frame of this.frames) {
        const label = document.createElement('label'); const input = document.createElement('input'); input.type = 'checkbox'; input.value = frame.frame_id; input.checked = this.chosen.has(frame.frame_id);
        const text = document.createElement('span'); text.textContent = frame.place_name;
        input.addEventListener('change', () => {
          if (input.checked) this.chosen.add(frame.frame_id); else this.chosen.delete(frame.frame_id);
          this.updateSummary(); this.onChange();
        });
        label.append(input,text); choices.append(label);
      }
      this.root.append(choices);
    }
    const summary = document.createElement('p'); summary.className = 'destination-summary'; summary.setAttribute('role','status'); this.root.append(summary);
    this.updateSummary();
  }
  updateSummary() {
    this.root.querySelector('.destination-summary').textContent = this.summary || 'Choose at least one frame.';
  }
}
