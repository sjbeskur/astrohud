import assert from 'node:assert/strict';
const {default:puppeteer}=await import(process.env.PUPPETEER_MODULE || 'puppeteer');
const base=process.env.PLAYBACK_TEST_URL || 'http://127.0.0.1:18115';
assert(['127.0.0.1','localhost'].includes(new URL(base).hostname));
const browser=await puppeteer.launch({executablePath:process.env.CHROME_PATH || '/usr/bin/google-chrome',headless:true,args:['--no-sandbox']});
try {
 const page=await browser.newPage();page.setDefaultTimeout(10000);console.log('QA: browser started');const errors=[];page.on('pageerror',error=>errors.push(error.message));
 await page.setViewport({width:390,height:844,isMobile:true,hasTouch:true});
 await page.goto(base+'/owner.html');
 const fixture=await page.evaluate(async()=>{
  const device={device_id:crypto.randomUUID(),device_code:'AB23CD',credential:crypto.randomUUID()+crypto.randomUUID(),bootstrap_token:crypto.randomUUID()+crypto.randomUUID()};
  async function post(path,body){const r=await fetch(path,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});if(!r.ok)throw Error(await r.text());return r.json();}
  await post('/api/beta/devices/enrollments',device);
  await post('/api/beta/bootstrap/claim',{token:device.bootstrap_token,place_name:'Playback QA'});
  const photos=[];
  for(let i=0;i<3;i++){
   const canvas=document.createElement('canvas');canvas.width=80;canvas.height=60;const ctx=canvas.getContext('2d');ctx.fillStyle=['#efb46a','#70a9d6','#ad96d8'][i];ctx.fillRect(0,0,80,60);ctx.fillStyle='#000';ctx.fillText(String(i),20,30);
   const blob=await new Promise(resolve=>canvas.toBlob(resolve,'image/jpeg'));
   const form=new FormData();form.append('photo',blob,`sample-${i}.jpg`);
   const r=await fetch(`/api/beta/owner/frames/${device.device_id}/photos`,{method:'POST',body:form});if(!r.ok)throw Error(await r.text());photos.push((await r.json()).photo_id);
  }
  return {device,photos};
 });
 const playbackUrl=`/api/beta/owner/frames/${fixture.device.device_id}/playback`;
 console.log('QA: fixture enrolled and photos uploaded');
 await page.goto(base+'/owner.html?qa=playback#device-settings');
 await page.waitForFunction(()=>Object.hasOwn(slideshowPlayback,'slide_seconds')&&!slideshowLoading);
 assert(await page.$eval('#playback-mode',select=>select.disabled));
 console.log('QA: legacy control disabled');
 const devicePage=await browser.newPage();devicePage.setDefaultTimeout(10000);devicePage.on('pageerror',error=>errors.push(error.message));
 await devicePage.evaluateOnNewDocument(device=>sessionStorage.setItem('astrohud-simulated-device',JSON.stringify(device)),fixture.device);
 await devicePage.goto(base+'/device-simulator.html');
 await devicePage.waitForFunction(()=>typeof slideshow!=='undefined'&&slideshow.currentId&&!slideshow.busy);
 await devicePage.evaluate(()=>{clearInterval(slideshow.timer);clearTimeout(manifestTimer);});
 assert(await devicePage.evaluate(()=>slideshow.photos.every(photo=>Number.isInteger(photo.upload_order))), 'manifest includes upload-order tie breakers');
 console.log('QA: simulator rendered first photo');
 await page.bringToFront();
 await page.evaluate(()=>loadSlideshowSettings());
 for(const width of [320,390,1280]){
  await page.setViewport({width,height:900,isMobile:width<500,hasTouch:width<500});
  await page.waitForFunction(()=>!slideshowLoading);
  await page.waitForFunction(()=>document.querySelector('#playback-mode').offsetHeight>0);
  assert(!await page.$eval('#playback-mode',s=>s.disabled));
  assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`overflow at ${width}`);
 }
 console.log('QA: layouts passed');
 await page.focus('input[name="playback-mode"][value="fresh_prints"]');
 await page.keyboard.press('Space');
 await page.waitForFunction(()=>slideshowPlayback.playback_mode==='fresh_prints'&&!slideshowBusy);
 assert.equal(await page.$eval('input[name="playback-mode"]:checked',input=>input.value),'fresh_prints');
 const blockSave=request=>request.method()==='POST'&&request.url().endsWith('/playback')?request.abort():request.continue();
 await page.setRequestInterception(true);page.on('request',blockSave);
 await page.click('input[name="playback-mode"][value="serendipity"]');
 await page.waitForFunction(()=>!slideshowBusy&&document.querySelector('#slideshow-feedback').textContent.includes('Could not save'));
 assert.equal(await page.$eval('input[name="playback-mode"]:checked',input=>input.value),'fresh_prints','a failed save restores the persisted choice');
 assert(await page.$$eval('input[name="playback-mode"]',inputs=>inputs.every(input=>!input.matches(':disabled'))));
 await page.setRequestInterception(false);page.off('request',blockSave);
 console.log('QA: keyboard selection and failed-save recovery passed');
 async function select(mode){
  console.log('QA: selecting',mode);
  await page.click(`input[name="playback-mode"][value="${mode}"]`);
  await page.waitForFunction(mode=>slideshowPlayback.playback_mode===mode&&!slideshowBusy,{},mode);
  assert.equal(await page.$eval('input[name="playback-mode"]:checked',input=>input.value),mode);
  await devicePage.evaluate(async device=>{const headers={Authorization:`Bearer ${device.credential}`,'X-Memory-Lane-Playback-Modes':'1'};const r=await fetch('/api/beta/device/manifest',{headers});await slideshow.update(await r.json(),headers);},fixture.device);
 }
 async function draw(){return devicePage.evaluate(async()=>{slideshow.nextAt=0;await slideshow.tick();return slideshow.currentId;});}
 await select('fresh_prints');const newest=await devicePage.evaluate(()=>slideshow.photos.map(p=>p.id));
 for(const expected of newest)assert.equal(await draw(),expected);
 await select('serendipity');
 await devicePage.evaluate(()=>{slideshow.order.random=()=>0.999;slideshow.order.pending=null;});
 assert.equal(await draw(),newest.at(-1));assert.equal(await draw(),newest.at(-1));
 await select('scenic_route');const first=await draw();
 console.log('QA: restarting simulator');
 await devicePage.bringToFront();
 await devicePage.reload();
 await devicePage.waitForFunction(()=>slideshow.currentId&&!slideshow.busy);
 const restored=await devicePage.evaluate(()=>{clearInterval(slideshow.timer);clearTimeout(manifestTimer);return slideshow.currentId;});
 assert.notEqual(restored,first,'Scenic progress must survive a browser restart');
 await page.evaluate(async({url,photo})=>{const r=await fetch(url,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({action:'pin',photo_id:photo,pin_seconds:0})});if(!r.ok)throw Error(await r.text());},{url:playbackUrl,photo:newest[0]});
 await devicePage.evaluate(async device=>{const headers={Authorization:`Bearer ${device.credential}`,'X-Memory-Lane-Playback-Modes':'1'};const r=await fetch('/api/beta/device/manifest',{headers});await slideshow.update(await r.json(),headers);},fixture.device);
 assert.equal(await draw(),newest[0]);assert.equal(await draw(),newest[0]);
 // An older binary's next manifest request must disable unsupported controls.
 await page.evaluate(async device=>{const r=await fetch('/api/beta/device/manifest',{headers:{Authorization:`Bearer ${device.credential}`}});if(!r.ok)throw Error(await r.text());await loadSlideshowSettings();},fixture.device);
 assert(await page.$eval('#playback-mode',s=>s.disabled));
 await page.evaluate(async device=>{await fetch('/api/beta/device/manifest',{headers:{Authorization:`Bearer ${device.credential}`,'X-Memory-Lane-Playback-Modes':'1'}});await loadSlideshowSettings();},fixture.device);
 assert(!await page.$eval('#playback-mode',s=>s.disabled));
 await page.bringToFront();
 await page.setViewport({width:390,height:900,isMobile:true,hasTouch:true});
 await page.waitForFunction(()=>!slideshowLoading);
 assert.equal(await page.$('#open-slideshow'),null);
 assert(await page.$eval('#slideshow-settings',element=>element.closest('#device-settings')!==null));
 await (await page.$('#slideshow-settings')).screenshot({path:'/tmp/memory-lane-playback-modes.png'});
 assert.deepEqual(errors,[]);
 console.log('PASS: selector, per-frame persistence, legacy viewer gating/rollback, real image playback in all modes, Scenic restart progress, random repeats, owner pins and 320/390/1280px layouts');
} finally {await browser.close();}
