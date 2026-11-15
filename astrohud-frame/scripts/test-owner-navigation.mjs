import assert from 'node:assert/strict';
const {default:puppeteer}=await import(process.env.PUPPETEER_MODULE || 'puppeteer');
const base=process.env.STORAGE_TEST_URL || 'http://127.0.0.1:18116';
assert(['127.0.0.1','localhost'].includes(new URL(base).hostname));
const browser=await puppeteer.launch({executablePath:process.env.CHROME_PATH || '/usr/bin/google-chrome',headless:true,args:['--no-sandbox']});
try {
  const page=await browser.newPage();page.setDefaultTimeout(10000);
  const errors=[];page.on('pageerror',error=>errors.push(error.message));
  await page.goto(base+'/owner.html');
  assert(await page.$eval('#owner-navigation',element=>element.hidden));
  const devices=await page.evaluate(async()=>{
    async function post(path,body){const response=await fetch(path,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});if(!response.ok)throw Error(await response.text());return response.json();}
    const devices=[];
    for(const name of ['Living room','Kitchen']){
      const device={device_id:crypto.randomUUID(),device_code:'AB23CD',credential:crypto.randomUUID()+crypto.randomUUID(),bootstrap_token:crypto.randomUUID()+crypto.randomUUID()};
      await post('/api/beta/devices/enrollments',device);
      await post(devices.length?'/api/beta/owner/devices/claim':'/api/beta/bootstrap/claim',{token:device.bootstrap_token,place_name:name});
      devices.push(device);
    }
    return devices;
  });
  await page.goto(base+'/owner.html?qa=navigation#home');
  await page.waitForFunction(()=>!document.querySelector('#owner-navigation').hidden);
  await page.select('#active-frame',devices[1].device_id);
  await page.evaluate(async device=>{
    const response=await fetch('/api/beta/device/updates',{method:'POST',headers:{Authorization:`Bearer ${device.credential}`,'Content-Type':'application/json'},body:JSON.stringify({updater_version:'0.1.1',installed_release:'2-0.5.3',available_release:'3-0.5.4',available_version:'0.5.4',metadata_expires_unix:Math.floor(Date.now()/1000)+3600})});
    if(!response.ok)throw Error(await response.text());await softwareUpdates.refresh();
  },devices[1]);
  assert(!await page.$eval('#home-update-badge',element=>element.hidden));
  for(const width of [320,390,1280]){
    await page.setViewport({width,height:844,isMobile:width<500,hasTouch:width<500});
    for(const view of ['photos-panel','owner-send-panel','invite-panel','device-settings','connected-panel']){
      await page.click(`.owner-nav-link[data-view="${view}"]`);
      await page.waitForFunction(view=>!document.getElementById(view).hidden,{},view);
      assert.equal(await page.$eval('.owner-nav-link[aria-current="page"]',element=>element.dataset.view),view);
      assert.equal(await page.$eval('#active-frame',element=>element.value),devices[1].device_id);
      assert(await page.$$eval('.owner-nav-link',links=>links.every(link=>{const r=link.getBoundingClientRect();return r.width>=44&&r.height>=44;})));
      assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`navigation overflow at ${width}`);
    }
    await page.click('#open-device-settings');
    await page.waitForFunction(()=>!document.querySelector('#device-settings').hidden);
    await page.evaluate(()=>window.scrollTo(0,document.body.scrollHeight));
    await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
    assert(await page.$eval('#owner-navigation',element=>Math.abs(element.getBoundingClientRect().top)<1));
    assert(await page.$eval('#owner-home-link',link=>{const r=link.getBoundingClientRect();return document.elementFromPoint(r.x+r.width/2,r.y+r.height/2)?.closest('a')===link;}),'Home stays visible and tappable after scrolling');
    await page.click('#owner-home-link');
    await page.waitForFunction(()=>!document.querySelector('#connected-panel').hidden&&window.scrollY===0);
    assert(!await page.$eval('#home-update-badge',element=>element.hidden));
  }
  await page.click('#open-device-settings');
  await page.waitForFunction(()=>!slideshowLoading&&Object.hasOwn(slideshowPlayback,'slide_seconds'));
  await page.click('[data-slide-seconds="20"]');
  await page.waitForFunction(()=>!slideshowBusy&&slideshowPlayback.slide_seconds===20);
  await page.click('#blur-background');
  await page.waitForFunction(()=>!slideshowBusy&&slideshowPlayback.blur_background===false);
  await page.click('#guest-display-next');
  await page.waitForFunction(()=>!slideshowBusy&&slideshowPlayback.guest_display_next===true);
  await page.select('#active-frame',devices[0].device_id);
  await page.waitForFunction(id=>slideshowFrameId===id&&!slideshowLoading&&slideshowPlayback.slide_seconds===12,{},devices[0].device_id);
  assert(await page.$eval('#blur-background',input=>input.checked));
  assert(!await page.$eval('#guest-display-next',input=>input.checked));
  // Hold a save for one frame, switch frames, then release the old response.
  const captured=Promise.withResolvers();
  const holdSave=request=>request.method()==='POST'&&request.url().includes(`/frames/${devices[0].device_id}/playback`)?captured.resolve(request):request.continue();
  await page.setRequestInterception(true);page.on('request',holdSave);
  await page.click('[data-slide-seconds="5"]');
  const held=await Promise.race([captured.promise,new Promise((_,reject)=>{setTimeout(()=>reject(Error('Settings save was not captured')),10000).unref();})]);
  await page.select('#active-frame',devices[1].device_id);
  await page.waitForFunction(id=>slideshowFrameId===id&&!slideshowLoading&&slideshowPlayback.slide_seconds===20,{},devices[1].device_id);
  await held.continue();
  await page.waitForNetworkIdle({idleTime:100,timeout:5000});
  assert.equal(await page.evaluate(()=>slideshowPlayback.slide_seconds),20);
  assert(!await page.$eval('#blur-background',input=>input.checked));
  assert(await page.$eval('#guest-display-next',input=>input.checked));
  assert.equal(await page.evaluate(async id=>(await (await fetch(`/api/beta/owner/frames/${id}/playback`)).json()).slide_seconds,devices[0].device_id),5);
  await page.setRequestInterception(false);page.off('request',holdSave);
  const blockLoad=request=>request.method()==='GET'&&request.url().includes(`/frames/${devices[0].device_id}/playback`)?request.abort():request.continue();
  await page.setRequestInterception(true);page.on('request',blockLoad);
  await page.select('#active-frame',devices[0].device_id);
  await page.waitForFunction(()=>!slideshowLoading&&!document.querySelector('#retry-slideshow').hidden);
  assert(await page.$$eval('#display-form input, #display-form button',inputs=>inputs.every(input=>input.matches(':disabled'))));
  await page.setRequestInterception(false);page.off('request',blockLoad);
  await page.click('#retry-slideshow');
  await page.waitForFunction(()=>!slideshowLoading&&slideshowPlayback.slide_seconds===5);
  console.log('Settings checks passed: timing, background, guest control, per-frame settings, stale-save isolation, load failure and retry.');
  await page.click('#owner-home-link');
  await page.waitForFunction(()=>!document.querySelector('#connected-panel').hidden);
  await page.click('.owner-nav-link[data-view="invite-panel"]');
  await page.waitForFunction(()=>!document.querySelector('#invite-panel').hidden);
  await page.goBack();
  await page.waitForFunction(()=>!document.querySelector('#connected-panel').hidden);
  assert.equal(await page.$eval('.owner-nav-link[aria-current="page"]',element=>element.dataset.view),'connected-panel');
  await page.setViewport({width:390,height:844,isMobile:true,hasTouch:true});
  await page.screenshot({path:'/tmp/memory-lane-owner-navigation.png'});
  assert.deepEqual(errors,[]);
  console.log('Owner navigation checks passed: all destinations, sticky/tappable Home, active states, selected frame, update badge, browser Back, signed-out gating, and 320/390/1280px layouts.');
} finally {await browser.close();}
