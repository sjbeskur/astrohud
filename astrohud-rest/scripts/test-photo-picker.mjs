import assert from 'node:assert/strict';
import {mkdtemp,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import path from 'node:path';
const {default:puppeteer}=await import(process.env.PUPPETEER_MODULE||'puppeteer');
const base=process.env.STORAGE_TEST_URL||'http://127.0.0.1:18116';
assert(['127.0.0.1','localhost'].includes(new URL(base).hostname));
const directory=await mkdtemp(path.join(tmpdir(),'memory-lane-picker-'));
const png=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEElEQVR4nGNwaDgARAwQCgAoDgYBQzpj2gAAAABJRU5ErkJggg==','base64');
const files=[path.join(directory,'first.png'),path.join(directory,'second.png')];
for(const file of files)await writeFile(file,png);
const browser=await puppeteer.launch({executablePath:process.env.CHROME_PATH||'/usr/bin/google-chrome',headless:true,args:['--no-sandbox']});
try{
  const page=await browser.newPage();page.setDefaultTimeout(10000);
  const errors=[];page.on('pageerror',error=>errors.push(error.message));
  await page.goto(base+'/owner.html');
  const fixture=await page.evaluate(async()=>{
    async function post(url,body){const r=await fetch(url,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});if(!r.ok)throw Error(await r.text());return r.json();}
    const ids=[];
    for(const name of ['Living room','Kitchen']){
      const device={device_id:crypto.randomUUID(),device_code:'AB23CD',credential:crypto.randomUUID()+crypto.randomUUID(),bootstrap_token:crypto.randomUUID()+crypto.randomUUID()};
      await post('/api/beta/devices/enrollments',device);
      await post(ids.length?'/api/beta/owner/devices/claim':'/api/beta/bootstrap/claim',{token:device.bootstrap_token,place_name:name});ids.push(device.device_id);
    }
    const invitation=await post('/api/beta/owner/invitations',{label:'Photo picker QA'});
    localStorage.setItem('memory-lane-selected-frame',ids[0]);
    return {ids,token:invitation.token};
  });
  for(const role of ['owner','sender']){
    if(role==='owner')await page.goto(base+'/owner.html?qa=picker#owner-send-panel');
    else await page.goto(base+'/sender-activate.html#'+fixture.token);
    const card=role==='owner'?'#owner-choose-photos':'#sender-choose-photos';
    const preview=role==='owner'?'#owner-preview-shell':'#preview-shell';
    const form=role==='owner'?'#owner-photo-form':'#photo-form';
    const status=role==='owner'?'#owner-send-status':'#status';
    await page.waitForSelector(card,{visible:true});
    for(const width of [320,390,1280]){
      await page.setViewport({width,height:900,isMobile:width<500,hasTouch:width<500});
      assert(await page.$eval(card,button=>button.getBoundingClientRect().height>=140));
      assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`${role} overflow ${width}`);
    }
    await page.setViewport({width:390,height:900,isMobile:true,hasTouch:true});
    if(role==='owner')await page.screenshot({path:'/tmp/memory-lane-photo-picker.png'});
    assert(await page.$eval(form+' button[type=submit]',button=>button.disabled));
    async function choose(file){const pending=page.waitForFileChooser();await page.click(card);const chooser=await pending;if(file)await chooser.accept([file]);else await chooser.cancel();}
    await choose(files[0]);
    await page.waitForFunction(selector=>document.querySelector(selector).querySelectorAll('figure').length===1,{},preview);
    assert.match(await page.$eval(card,e=>e.textContent),/Add more photos/);
    await choose(files[1]);
    await page.waitForFunction(selector=>document.querySelector(selector).querySelectorAll('figure').length===2,{},preview);
    await choose(files[1]);
    assert.equal(await page.$$eval(preview+' figure',items=>items.length),2,'choosing the same file does not duplicate it');
    await page.click(preview+' .photo-remove');
    assert.equal(await page.$$eval(preview+' figure',items=>items.length),1);
    await choose(null);
    assert.equal(await page.$$eval(preview+' figure',items=>items.length),1,'cancelling preserves the queue');
    assert.match(await page.$eval(form+' button[type=submit]',e=>e.textContent),/Send 1 photo to Living room/);
    await page.click(form+' [data-destination-mode="all"]');
    assert.match(await page.$eval(form+' button[type=submit]',e=>e.textContent),/Send 1 photo to 2 frames/);
    await page.click(form+' button[type=submit]');
    await page.waitForFunction(selector=>document.querySelector(selector).textContent.includes('1 photo received'),{},status);
    assert.match(await page.$eval(card,e=>e.textContent),/Choose photos/);
    assert(await page.$eval(preview,e=>e.hidden));
    console.log(`${role}: picker activation, add-more, duplicates, remove, cancel, destination label and upload passed`);
  }
  assert.deepEqual(errors,[]);
}finally{await browser.close();await rm(directory,{recursive:true,force:true});}
