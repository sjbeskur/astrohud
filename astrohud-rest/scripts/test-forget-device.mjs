import assert from 'node:assert/strict';
const {default:puppeteer}=await import(process.env.PUPPETEER_MODULE||'puppeteer');
const base=process.env.FORGET_TEST_URL||'http://127.0.0.1:18119';
assert(['127.0.0.1','localhost'].includes(new URL(base).hostname));
const browser=await puppeteer.launch({executablePath:process.env.CHROME_PATH||'/usr/bin/google-chrome',headless:true,args:['--no-sandbox']});
async function fixture(page,includeOthers){
 await page.goto(base+'/owner.html');
 return page.evaluate(async includeOthers=>{
  async function post(path,body,credential){
   const r=await fetch(path,{method:'POST',headers:{'Content-Type':'application/json',...(credential?{Authorization:'Bearer '+credential}:{})},body:JSON.stringify(body)});
   if(!r.ok)throw Error(await r.text());return r.status===204?null:r.json();
  }
  let first=true;
  async function create(name,state){
   const device={device_id:crypto.randomUUID(),device_code:'AB23CD',credential:crypto.randomUUID()+crypto.randomUUID(),bootstrap_token:crypto.randomUUID()+crypto.randomUUID()};
   await post('/api/beta/devices/enrollments',device);
   await post(first?'/api/beta/bootstrap/claim':'/api/beta/owner/devices/claim',{token:device.bootstrap_token,place_name:name});first=false;
   if(!state)return {id:device.device_id};
   const reset=await post('/api/beta/owner/frames/'+device.device_id+'/reset',{confirm_name:name});
   if(state!=='pending'){
    const receipt=crypto.randomUUID()+crypto.randomUUID();
    await post('/api/beta/device/resets/'+reset.reset_id+'/accept',{receipt,next_credential:crypto.randomUUID()+crypto.randomUUID()},device.credential);
    if(state==='complete')await post('/api/beta/device/resets/'+reset.reset_id+'/complete',{},receipt);
   }
   return {id:device.device_id,reset:reset.reset_id};
  }
  const live=includeOthers?await create('Living room'):null;
  const waiting=includeOthers?await create('Waiting for reset','pending'):null;
  const accepted=await create('Old frame','resetting');
  const complete=includeOthers?await create('Finished frame','complete'):null;
  return {live,waiting,accepted,complete};
 },includeOthers);
}
const card=id=>`.reset-progress-card[data-reset-id="${id}"]`;
try{
 const context=await browser.createBrowserContext();const page=await context.newPage();
 const errors=[];page.on('pageerror',e=>errors.push(e.message));page.setDefaultTimeout(10000);
 const f=await fixture(page,true);
 await page.goto(base+'/owner.html#device-settings');await page.reload();
 try {await page.waitForSelector(card(f.accepted.reset)+' button');}catch(error){
  console.log('UI diagnostics',errors,await page.evaluate(async()=>{const c=await fetch('/api/beta/owner').then(r=>r.json());return {version:c.reset_forget_version,frames:c.frames?.length,resets:c.resets?.map(r=>({id:r.reset_id,state:r.state})),cards:document.querySelectorAll('.reset-progress-card').length,buttons:[...document.querySelectorAll('.reset-progress-card button')].map(b=>b.textContent)};}));throw error;
 }
 assert.equal(await page.$eval(card(f.waiting.reset),e=>e.querySelector('button')),null);
 for(const width of [320,390,1280]){
  await page.setViewport({width,height:900,isMobile:width<500,hasTouch:width<500});
  assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'overflow '+width);
  assert(await page.$eval(card(f.accepted.reset)+' button',e=>e.getBoundingClientRect().height>=44));
 }
 await page.setViewport({width:390,height:900,isMobile:true,hasTouch:true});
 if(process.env.FORGET_SCREENSHOT)await page.screenshot({path:process.env.FORGET_SCREENSHOT,fullPage:true});
 page.once('dialog',d=>d.dismiss());await page.click(card(f.accepted.reset)+' button');
 assert(await page.$(card(f.accepted.reset)),'cancel must keep device');
 const oldContext=await page.evaluate(async()=>fetch('/api/beta/owner').then(r=>r.json()));
 let delayed;let hold=true;
 await page.setRequestInterception(true);
 page.on('request',r=>{if(hold&&r.url()===base+'/api/beta/owner'&&r.method()==='GET'){hold=false;delayed=r;}else r.continue();});
 const deadline=Date.now()+8000;while(!delayed&&Date.now()<deadline)await new Promise(r=>setTimeout(r,50));
 assert(delayed,'expected an in-flight reset poll');
 page.once('dialog',d=>d.accept());await page.click(card(f.accepted.reset)+' button');
 await page.waitForFunction(selector=>!document.querySelector(selector),{},card(f.accepted.reset));
 await delayed.respond({status:200,contentType:'application/json',body:JSON.stringify(oldContext)});
 await new Promise(r=>setTimeout(r,150));
 assert.equal(await page.$(card(f.accepted.reset)),null,'stale poll must not restore forgotten device');
 const after=await page.evaluate(async()=>fetch('/api/beta/owner').then(r=>r.json()));
 assert(after.frames.some(x=>x.frame_id===f.live.id));assert(!after.frames.some(x=>x.frame_id===f.accepted.id));
 assert(after.resets.some(x=>x.reset_id===f.waiting.reset));
 // New UI must not advertise this action against an older hosted API.
 await page.evaluate(()=>{const old=structuredClone(ownerContext);delete old.reset_forget_version;renderDeviceSettings(old);});
 assert.equal(await page.$(card(f.complete.reset)+' button'),null);
 await page.reload();await page.waitForSelector(card(f.complete.reset)+' button');
 page.once('dialog',d=>d.accept());await page.click(card(f.complete.reset)+' button');
 await page.waitForFunction(selector=>!document.querySelector(selector),{},card(f.complete.reset));
 assert.deepEqual(errors,[]);await context.close();
 const onlyContext=await browser.createBrowserContext();const only=await onlyContext.newPage();
 const single=await fixture(only,false);await only.goto(base+'/owner.html#device-settings');await only.reload();
 await only.waitForSelector(card(single.accepted.reset)+' button');
 only.once('dialog',d=>d.accept());await only.click(card(single.accepted.reset)+' button');
 await only.waitForFunction(selector=>!document.querySelector(selector),{},card(single.accepted.reset));
 const empty=await only.evaluate(async()=>fetch('/api/beta/owner').then(r=>r.json()));
 assert.deepEqual(empty.frames,[]);assert.deepEqual(empty.resets,[]);
 assert(await only.$eval('#device-settings',e=>!e.hidden));
 console.log('Passed: pending/accepted/completed actions, cancel, stale polls, older API compatibility, last-device removal and 320/390/1280px layouts.');
}finally{await browser.close();}
