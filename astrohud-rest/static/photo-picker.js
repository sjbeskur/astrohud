/* A clear entry point shared by owner and invited-sender photo forms. */
class MemoryLanePhotoPicker {
  constructor(input, button) {
    this.input = input;
    this.button = button;
    button.addEventListener('click', () => {
      // Queued File objects remain valid; resetting lets the same file be picked again.
      input.value = '';
      input.click();
    });
    this.setCount(0);
  }
  setCount(count) {
    this.button.dataset.selected = String(count > 0);
    this.button.querySelector('strong').textContent = count ? 'Add more photos' : 'Choose photos';
    this.button.querySelector('[data-picker-description]').textContent = count
      ? `${count} ${count === 1 ? 'photo' : 'photos'} selected · Pick more from your library`
      : 'Pick from your photo library';
  }
  static append(queue, files) {
    const key = file => JSON.stringify([file.name, file.size, file.lastModified, file.type]);
    const known = new Set(queue.map(item => key(item.file)));
    const next = [...queue];
    for (const file of files) {
      if (known.has(key(file))) continue;
      known.add(key(file));
      const error = file.size > 20 * 1024 * 1024 ? 'Over 20 MB — choose a smaller photo.'
        : !['image/jpeg', 'image/png'].includes(file.type) ? 'Choose a JPEG or PNG photo.' : '';
      next.push({file, url: URL.createObjectURL(file), error, invalid: Boolean(error)});
    }
    return next;
  }
  static sendLabel(count, recipients) {
    const target = recipients.length === 1 ? recipients[0].place_name : `${recipients.length} frames`;
    return count ? `Send ${count} ${count === 1 ? 'photo' : 'photos'} to ${target}` : 'Send photos';
  }
}
