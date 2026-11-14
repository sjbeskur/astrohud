// Run against a disposable local server. Creates only synthetic households.
// PUPPETEER_MODULE may point to an existing Puppeteer ESM module installation.
import assert from 'node:assert/strict';
const {default: puppeteer} = await import(process.env.PUPPETEER_MODULE || 'puppeteer');
const base = process.env.UPDATER_TEST_URL || 'http://127.0.0.1:18089';
const origin = new URL(base);
assert(['127.0.0.1', 'localhost'].includes(origin.hostname), 'Use an isolated local test server');
const browser = await puppeteer.launch({executablePath: process.env.CHROME_PATH || '/usr/bin/google-chrome', headless: true, args: ['--no-sandbox']});
try {
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', error => { errors.push(error.message); console.error('Page error:', error.message); });
  page.on('dialog', dialog => dialog.accept());
  await page.goto(`${base}/owner.html`);
  const device = await page.evaluate(async () => {
    const id = crypto.randomUUID();
    const credential = crypto.randomUUID() + crypto.randomUUID();
    const token = crypto.randomUUID() + crypto.randomUUID();
    const enroll = await fetch('/api/beta/devices/enrollments', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify({device_id: id, device_code: 'AB23CD', credential, bootstrap_token: token})});
    if (!enroll.ok) throw new Error(await enroll.text());
    const claim = await fetch('/api/beta/bootstrap/claim', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify({token, place_name: 'QA Kitchen'})});
    if (!claim.ok) throw new Error(await claim.text());
    return {id, credential};
  });
  await page.goto(`${base}/owner.html?qa=1#device-settings`);
  await page.waitForFunction(() => document.querySelector('#software-update').textContent.includes('not available'), {timeout: 10000}).catch(async error => { console.error(await page.evaluate(() => ({status: document.querySelector('#status').textContent, updates: document.querySelector('#software-update').textContent, view: location.hash}))); throw error; });
  assert(await page.$eval('#software-update button', b => b.disabled));
  const report = {updater_version: '0.1.0', installed_release: 'baseline', previous_release: null,
    available_release: '6-0.6.0', available_version: '0.6.0', checked_unix: Math.floor(Date.now()/1000), metadata_expires_unix: Math.floor(Date.now()/1000)+3600};
  async function heartbeat(extra = {}) {
    return page.evaluate(async ({device, report, extra}) => {
      const response = await fetch('/api/beta/device/updates', {method: 'POST', headers: {'Content-Type': 'application/json', Authorization: `Bearer ${device.credential}`}, body: JSON.stringify({...report, ...extra})});
      if (!response.ok) throw new Error(await response.text());
      return response.json();
    }, {device, report, extra});
  }
  await heartbeat();
  await page.evaluate(() => softwareUpdates.refresh());
  await page.waitForFunction(() => !document.querySelector('#software-update .signal-button').hidden);
  await page.evaluate(() => { location.hash = 'home'; });
  await page.waitForFunction(() => document.querySelector('#home-update-badge').offsetHeight > 0);
  await page.click('#open-device-settings');
  await page.waitForFunction(() => location.hash === '#device-settings' && document.querySelector('#software-update').offsetHeight > 0);
  for (const width of [320, 390, 1280]) {
    await page.setViewport({width, height: 900, isMobile: width < 500, hasTouch: width < 500});
    await page.waitForFunction(() => document.querySelector('#software-update .update-install')?.offsetHeight > 0);
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `horizontal overflow at ${width}px`);
    const buttons = await page.$$eval('#software-update button', buttons => buttons.filter(b => !b.hidden).map(b => ({label: b.textContent, height: b.getBoundingClientRect().height})));
    assert(buttons.every(b => b.height >= 44), `touch targets at ${width}px`);
  }
  await page.setViewport({width: 390, height: 900, isMobile: true, hasTouch: true});
  await page.click('#software-update .signal-button');
  await page.waitForFunction(() => document.querySelector('#software-update').textContent.includes('Waiting for the frame'));
  const {command} = await heartbeat();
  assert.equal(command.action, 'update');
  assert.equal(command.release_id, '6-0.6.0');
  await heartbeat({request_id: command.id, request_state: 'running'});
  await page.evaluate(() => softwareUpdates.refresh());
  await page.waitForFunction(() => document.querySelector('#software-update').textContent.includes('Installing the update'));
  assert(await page.$eval('#software-update .signal-button', button => button.disabled));
  await heartbeat({installed_release: '6-0.6.0', previous_release: 'baseline', request_id: command.id, request_state: 'succeeded', message: 'Application update complete.'});
  await page.evaluate(() => softwareUpdates.refresh());
  await page.waitForFunction(() => document.querySelector('#software-update').textContent.includes('up to date'));
  assert(await page.$eval('#software-update .signal-button', button => button.hidden));
  assert(await page.$eval('#home-update-badge', badge => badge.hidden));
  const rollback = await page.$$('#software-update button');
  await rollback[2].click();
  await page.waitForFunction(() => document.querySelector('#software-update').textContent.includes('Waiting for the frame'));
  const response = await heartbeat({installed_release: '6-0.6.0', previous_release: 'baseline'});
  assert.equal(response.command.action, 'rollback');
  assert.equal(response.command.release_id, 'baseline');
  await heartbeat({installed_release: '6-0.6.0', previous_release: 'baseline', request_id: response.command.id, request_state: 'failed', message: 'Could not restore the previous release.'});
  await page.evaluate(() => softwareUpdates.refresh());
  await page.waitForFunction(() => document.querySelector('#software-update').textContent.includes('Could not restore'));
  await page.screenshot({path: '/tmp/memory-lane-updater-mobile.png', fullPage: true});
  assert.deepEqual(errors, []);
  console.log('PASS: headless owner flow, unsupported device, exact-version install, progress, rollback failure, 320/390/1280px layouts and touch targets');
} finally {
  await browser.close();
}
