import {resolve, dirname, join} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
const {default:puppeteer}=await import(process.env.PUPPETEER_MODULE||'puppeteer');
const directory=dirname(fileURLToPath(import.meta.url));
const variants={
  original:{suffix:'',preview:'preview',asset:null},
  black:{suffix:'-Black-Family',preview:'preview-black-family',asset:'family-photo-black.png',alt:'Illustrative family photograph of a Black grandmother and grandchild, shown on a TV'},
  indian:{suffix:'-Indian-Family',preview:'preview-indian-family',asset:'family-photo-indian.png',alt:'Illustrative family photograph of an Indian grandmother and grandchild, shown on a TV'},
  dude:{position:'center top',suffix:'-The-Dude',preview:'preview-the-dude',asset:'the-dude.png',alt:'AI-generated illustration of the Dude relaxing beside a patterned rug and bowling ball, shown on a TV',lead:'A slideshow that really ties the room together. Families send photos from a phone, and Memory Lane brings them to the TV or monitor already in the room.',heroNote:'Works with a TV or monitor that has an HDMI input. AI-generated character illustration; unofficial themed concept.'}
};
variants['dude-family']={...variants.dude,suffix:'-The-Dude-Family',preview:'preview-the-dude-family',asset:'the-dude-family.png',alt:'AI-generated illustration of the Dude and his red-haired daughter sharing a relaxed moment on the sofa'};
const requested=process.argv.slice(2);
if(!requested.length)requested.push('original');
for(const name of requested)if(!Object.hasOwn(variants,name))throw Error('Unknown brochure variant: '+name);
const browser=await puppeteer.launch({executablePath:process.env.CHROME_PATH||'/usr/bin/google-chrome',headless:true,args:['--no-sandbox','--allow-file-access-from-files']});
try{
 for(const name of requested){
  const variant=variants[name];
  const page=await browser.newPage();
  await page.setViewport({width:900,height:1150,deviceScaleFactor:1.5});
  await page.goto(pathToFileURL(resolve(directory,'brochure.html')).href,{waitUntil:'networkidle0'});
  if(variant.asset)await page.evaluate(async variant=>{
    const image=document.querySelector('.tv img');
    image.src='assets/'+variant.asset;image.alt=variant.alt;
    if(variant.position)image.style.objectPosition=variant.position;
    if(variant.lead)document.querySelector('.lead').textContent=variant.lead;
    if(variant.heroNote)document.querySelector('.hero-note').textContent=variant.heroNote;
    await image.decode();
  },variant);
  await page.evaluate(()=>document.fonts.ready);
  const checks=await page.evaluate(()=>({
    pages:document.querySelectorAll('.page').length,
    images:[...document.images].map(image=>({file:image.getAttribute('src'),loaded:image.complete&&image.naturalWidth>0})),
    overflow:[...document.querySelectorAll('.page')].map(page=>page.scrollHeight>page.clientHeight),
    outside:[...document.querySelectorAll('.page')].flatMap(page=>{
      const bounds=page.getBoundingClientRect();
      return [...page.querySelectorAll('*')].filter(element=>{const r=element.getBoundingClientRect();return r.bottom>bounds.bottom+1||r.right>bounds.right+1||r.left<bounds.left-1;}).map(element=>element.tagName+'.'+element.className);
    })
  }));
  if(checks.pages!==2||checks.images.some(image=>!image.loaded)||checks.overflow.some(Boolean)||checks.outside.length)throw Error(JSON.stringify(checks));
  await page.pdf({path:join(directory,`Memory-Lane-Brochure${variant.suffix}.pdf`),preferCSSPageSize:true,printBackground:true,displayHeaderFooter:false});
  for(const [index,element] of (await page.$$('.page')).entries())await element.screenshot({path:join(directory,`${variant.preview}-page-${index+1}.png`)});
  console.log(JSON.stringify({variant:name,...checks}));
  await page.close();
 }
}finally{await browser.close();}
