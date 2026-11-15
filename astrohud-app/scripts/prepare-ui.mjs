import {mkdir,readFile,writeFile,copyFile} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
const app=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const source=path.resolve(app,'../astrohud-rest/static');
const output=path.join(app,'dist');
await mkdir(output,{recursive:true});
for(const name of ['owner.html','send.html','local-simulator.html']){
  let html=await readFile(path.join(source,name),'utf8');
  html=html.replace('</head>','<script src="/native-bridge.js"></script>\n</head>');
  html=html.replaceAll('<script>','<script type="module">\nawait window.memoryLaneReady;');
  html=html.replaceAll('${location.origin}','${window.memoryLaneServerOrigin}');
  html=html.replaceAll('image.src = photo.url;','window.memoryLanePhoto(image, photo.url);');
  html=html.replace(/(document\.querySelector\('#(?:photo-detail-image|sent-photo-image)'\))\.src = photo\.url;/g,'window.memoryLanePhoto($1, photo.url);');
  if(['owner.html','send.html'].includes(name)&&(html.match(/window\.memoryLanePhoto\(/g)||[]).length!==2)throw Error(`Review private-image bindings in ${name} before building.`);
  if(name==='local-simulator.html'){
    html=html.replace(/\s*<p><a class="quiet-button" href="\/background-preview.html">[^<]+<\/a><\/p>/,'');
    html=html.replace(/const local = [^;]+;/,'const local = window.memoryLaneLocalServer;');
    html=html.replace('This creates a separate demo household.','This creates a separate demo household. Sample access is kept only while this app is open.');
    // The simulator itself stays in the browser; the app contains the owner/sender UI.
    html=html.replace("sessionStorage.removeItem('astrohud-simulated-reset');location.href='/device-simulator.html';","sessionStorage.removeItem('astrohud-simulated-reset');location.href='/owner.html#photos-panel';");
    html=html.replace("button.textContent='View frame'","button.textContent='Open photos'");
    html=html.replace("sessionStorage.setItem('astrohud-simulated-device',JSON.stringify(frame.device));", "localStorage.setItem('memory-lane-selected-frame',frame.device.device_id);");
  }
  if(name==='send.html')html=html.replaceAll('href="/owner.html"','href="/index.html"');
  await writeFile(path.join(output,name),html);
}
for(const name of ['astrohud-theme.css','photo-destinations.js','photo-picker.js','device-updates.js','device-storage.js'])await copyFile(path.join(source,name),path.join(output,name));
for(const name of ['index.html','native-bridge.js','client.css','connect.js'])await copyFile(path.join(app,'ui',name),path.join(output,name));
console.log('Prepared bundled owner/sender UI from the shared website sources.');
