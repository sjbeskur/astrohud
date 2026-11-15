import assert from 'node:assert/strict';
const {default:puppeteer}=await import(process.env.PUPPETEER_MODULE || 'puppeteer');
const base=process.env.STORAGE_TEST_URL || 'http://127.0.0.1:18116';
assert(['127.0.0.1','localhost'].includes(new URL(base).hostname));
const browser=await puppeteer.launch({executablePath:process.env.CHROME_PATH || '/usr/bin/google-chrome',headless:true,args:['--no-sandbox']});
try {
  const page=await browser.newPage();page.setDefaultTimeout(10000);
  await page.setViewport({width:390,height:844,isMobile:true,hasTouch:true});
  const errors=[];page.on('pageerror',error=>errors.push(error.message));
  await page.goto(base+'/owner.html');
  const device=await page.evaluate(async()=>{
    const device={device_id:crypto.randomUUID(),device_code:'AB23CD',credential:crypto.randomUUID()+crypto.randomUUID(),bootstrap_token:crypto.randomUUID()+crypto.randomUUID()};
    for(const [path,body] of [['/api/beta/devices/enrollments',device],['/api/beta/bootstrap/claim',{token:device.bootstrap_token,place_name:'Storage QA'}]]){
      const r=await fetch(path,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});
      if(!r.ok)throw Error(await r.text());
    }
    return device;
  });
  await page.goto(base+'/owner.html?qa=storage#device-settings');
  await page.waitForFunction(()=>deviceStorage.data!==null);
  assert.match(await page.$eval('#device-storage',e=>e.textContent),/Waiting for storage/);
  const mib=1024*1024;
  const report={viewer_version:'0.4.0',disk_total_bytes:16000*mib,disk_used_bytes:2000*mib,disk_available_bytes:14000*mib,photo_bytes:100*mib,photo_count:100,photo_limit_bytes:1000*mib,reserve_bytes:512*mib};
  async function send(body){
    const status=await page.evaluate(async({body,device})=>{
      const r=await fetch('/api/beta/device/storage',{method:'POST',headers:{Authorization:`Bearer ${device.credential}`,'Content-Type':'application/json'},body:JSON.stringify(body)});
      await deviceStorage.refresh();return r.status;
    },{body,device});
    assert.equal(status,204);
  }
  await send(report);
  assert.match(await page.$eval('.storage-capacity',e=>e.textContent),/720 more photos/);
  assert(await page.$eval('#home-storage-notice',e=>e.hidden));
  await (await page.$('#device-storage')).screenshot({path:'/tmp/memory-lane-storage-preview.png'});
  for(const width of [320,390,1280]){
    await page.setViewport({width,height:900,isMobile:width<500,hasTouch:width<500});
    assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`storage overflow at ${width}`);
  }
  await send({...report,photo_count:0,photo_bytes:0});
  assert.match(await page.$eval('.storage-capacity',e=>e.textContent),/Add a few photos/);
  await send({...report,photo_count:1,photo_bytes:mib});
  assert.match(await page.$eval('#device-storage',e=>e.textContent),/early estimate/);
  await send({...report,photo_bytes:report.photo_limit_bytes});
  assert.equal(await page.$eval('#device-storage',e=>e.dataset.status),'healthy');
  assert(await page.$eval('#home-storage-notice',e=>e.hidden));
  await send({...report,disk_available_bytes:report.reserve_bytes});
  assert.match(await page.$eval('.storage-capacity',e=>e.textContent),/downloads paused/);
  assert(!await page.$eval('#home-storage-notice',e=>e.hidden));
  await send({...report,disk_available_bytes:50*mib});
  assert.equal(await page.$eval('#device-storage',e=>e.dataset.status),'very_low');
  await page.evaluate(()=>{deviceStorage.data.reported_unix=Date.now()/1000-7200;deviceStorage.paint();});
  assert.equal(await page.$eval('#device-storage',e=>e.dataset.status),'stale');
  assert(await page.$eval('#home-storage-notice',e=>e.hidden));
  assert.match(await page.$eval('.storage-updated',e=>e.textContent),/Last reported 2 hours ago/);
  await send(report);
  await page.setRequestInterception(true);
  page.on('request',request=>request.url().endsWith('/storage')&&request.method()==='GET'?request.abort():request.continue());
  await page.evaluate(()=>deviceStorage.refresh());
  assert.match(await page.$eval('.storage-capacity',e=>e.textContent),/At last report, room for about 720/);
  assert.match(await page.$eval('.storage-updated',e=>e.textContent),/Could not refresh/);
  // A late response for one frame must never overwrite another frame's reading.
  assert(await page.evaluate(async()=>{
    clearTimeout(deviceStorage.timer);
    const original=window.fetch;let resolve;
    window.fetch=()=>new Promise(r=>{resolve=r;});
    const old=deviceStorage.frame;
    const pending=deviceStorage.refresh();
    deviceStorage.setFrame(null);
    resolve({ok:true,json:async()=>({report:{photo_count:999}})});
    await pending;window.fetch=original;
    return deviceStorage.frame===null&&deviceStorage.data===null&&document.querySelector('#device-storage').hidden;
  }));
  assert.deepEqual(errors,[]);
  console.log('Storage browser checks passed: estimates, legacy, empty/early sample, cache full, disk reserve, critical, stale/error, frame switching, and mobile layouts.');
} finally { await browser.close(); }
