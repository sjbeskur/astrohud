// Browser regression checks with synthetic devices and fully intercepted traffic.
// First render portal previews with web::tests::write_portal_preview.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
const {default: puppeteer} = await import(process.env.PUPPETEER_MODULE || 'puppeteer');
const previewDir = process.env.ASTROHUD_PORTAL_PREVIEW_DIR || '/tmp/memory-lane-onboarding-preview';
const bootstrapToken = 'test-bootstrap-abcdefghijklmnopqrstuvwxyz0123456789';
const portal = (await readFile(`${previewDir}/astrohud-connecting-preview.html`, 'utf8')).replaceAll('#test-bootstrap', '#' + bootstrapToken);
const onboard = await readFile(new URL('../../astrohud-rest/static/onboard.html', import.meta.url), 'utf8');
const theme = await readFile(new URL('../../astrohud-rest/static/astrohud-theme.css', import.meta.url), 'utf8');
const browser = await puppeteer.launch({executablePath: process.env.CHROME_PATH || '/usr/bin/google-chrome', headless: true, args: ['--no-sandbox']});
try {
  for (const scenario of ['ready', 'older-api', 'failed-wifi']) {
    const context = await browser.createBrowserContext();
    const page = await context.newPage();
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    let ready = false;
    let handoffRequests = 0;
    let claims = 0;
    await page.setRequestInterception(true);
    page.on('request', async request => {
      const url = new URL(request.url());
      const cors = {'Access-Control-Allow-Origin': 'http://10.42.0.1', 'Access-Control-Allow-Methods': 'POST', 'Access-Control-Allow-Headers': 'Content-Type'};
      const json = (body, headers = {}, status = 200) => request.respond({status, contentType: 'application/json', headers, body: JSON.stringify(body)});
      if (request.method() === 'OPTIONS') return request.respond({status: 204, headers: cors});
      if (url.hostname === '10.42.0.1') {
        if (url.pathname === '/api/status') return json({state: scenario === 'failed-wifi' ? 'failed' : 'connected', message: 'Test connection failed'});
        return request.respond({status: 200, contentType: 'text/html', body: portal});
      }
      if (url.pathname === '/api/beta/bootstrap/handoff') {
        handoffRequests++;
        assert.equal(request.headers().cookie, undefined);
        assert.equal(JSON.parse(request.postData()).token, bootstrapToken);
        assert.equal(url.search, '');
        return json({status: ready ? 'ready' : 'waiting'}, cors, scenario === 'older-api' ? 404 : 200);
      }
      if (url.pathname === '/onboard.html') return request.respond({status: 200, contentType: 'text/html', body: onboard});
      if (url.pathname === '/astrohud-theme.css') return request.respond({status: 200, contentType: 'text/css', body: theme});
      if (url.pathname === '/api/beta/bootstrap/status') return json({status: 'ready', device_code: 'ABC2D3'});
      if (url.pathname === '/api/beta/owner') return json({household_name: 'Test family', frames: [{frame_id: 'existing'}]});
      if (url.pathname === '/api/beta/owner/devices/claim') { claims++; return json({}); }
      if (url.pathname === '/owner.html') return request.respond({status: 200, contentType: 'text/html', body: '<p>Owner page</p>'});
      return request.respond({status: 404, body: ''});
    });
    await page.goto('http://10.42.0.1/connecting');
    for (const width of [320, 390, 1280]) {
      await page.setViewport({width, height: 900});
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `${scenario}: overflow at ${width}`);
    }
    await page.waitForFunction(() => document.querySelector('#connection-title').textContent !== 'Connecting…');
    assert.equal(new URL(page.url()).hostname, '10.42.0.1', 'local success must not trigger navigation');
    if (scenario === 'failed-wifi') {
      assert(await page.$eval('#connection-next', element => element.hidden));
      ready = true;
      await new Promise(resolve => setTimeout(resolve, 3500));
      assert.equal(new URL(page.url()).hostname, '10.42.0.1', 'failed Wi-Fi must not continue');
    } else {
      await page.waitForFunction(() => document.querySelector('#connection-title').textContent === 'Wi-Fi saved');
      assert(await page.$eval('#continue-setup', element => element.href.includes('/onboard.html#test-bootstrap-')));
      const deadline = Date.now() + 10000;
      while (handoffRequests === 0 && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 50));
      assert(handoffRequests > 0, 'hosted readiness was polled');
      ready = true;
      if (scenario === 'older-api') {
        await new Promise(resolve => setTimeout(resolve, 3500));
        assert.equal(new URL(page.url()).hostname, '10.42.0.1');
        await page.click('#continue-setup');
      }
      await page.waitForSelector('#place-panel:not([hidden])');
      assert(await page.$eval('#household-summary', element => element.textContent.includes('Test family')));
      assert.equal(claims, 0);
      await page.type('#place-name', 'Kitchen');
      await page.click('#place-form button');
      assert.equal(claims, 0, 'existing household requires confirmation');
      await page.click('#confirm-household');
      await page.click('#place-form button');
      await page.waitForFunction(() => location.pathname === '/owner.html');
      assert.equal(claims, 1);
    }
    assert.deepEqual(errors, []);
    await context.close();
  }
  console.log('Passed: hosted readiness gating, old API fallback, failed Wi-Fi, household confirmation and responsive layouts.');
} finally { await browser.close(); }
