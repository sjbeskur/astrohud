/* A narrow native transport: server cookies never enter the webview. */
(() => {
  const invoke = (command,args) => window.__TAURI__.core.invoke(command,args);
  window.memoryLaneInvoke = invoke;
  window.memoryLaneReady = invoke('connection_info').then(info=>{
    window.memoryLaneServerOrigin=info.server;
    window.memoryLaneLocalServer=info.local;
    return info;
  });
  const originalFetch=window.fetch.bind(window);
  function encode(bytes){let text='';for(let i=0;i<bytes.length;i+=32768)text+=String.fromCharCode(...bytes.subarray(i,i+32768));return btoa(text);}
  function decode(text){const value=atob(text);return Uint8Array.from(value,c=>c.charCodeAt(0));}
  window.fetch=async(input,options={})=>{
    const raw=input instanceof Request?input.url:String(input);
    const url=new URL(raw,location.href);
    if(url.protocol!==location.protocol||url.host!==location.host||!url.pathname.startsWith('/api/beta/'))return originalFetch(input,options);
    await window.memoryLaneReady;
    const request=new Request(url,options);
    if(request.signal.aborted)throw new DOMException('Request cancelled','AbortError');
    const bytes=request.method==='GET'?null:new Uint8Array(await request.arrayBuffer());
    const result=await invoke('api_request',{request:{path:url.pathname+url.search,method:request.method,contentType:request.headers.get('content-type'),bodyBase64:bytes?.length?encode(bytes):null}});
    if(request.signal.aborted)throw new DOMException('Request cancelled','AbortError');
    return new Response([204,205,304].includes(result.status)?null:decode(result.bodyBase64),{status:result.status,headers:{'content-type':result.contentType}});
  };
  const photos=new Map();
  window.memoryLanePhoto=async(image,path)=>{
    const previous=photos.get(image);if(previous?.url)URL.revokeObjectURL(previous.url);
    const state={url:null};photos.set(image,state);image.removeAttribute('src');
    try{
      const response=await fetch(path);if(!response.ok)throw Error('Photo unavailable');
      const blob=await response.blob();if(photos.get(image)!==state)return;
      state.url=URL.createObjectURL(blob);image.src=state.url;
    }catch(_){if(photos.get(image)===state)image.alt='Photo temporarily unavailable';}
  };
  addEventListener('DOMContentLoaded',async()=>{
    const info=await window.memoryLaneReady;
    const bar=document.createElement('div');bar.className='native-connection';
    const text=document.createElement('span');text.textContent=info.local?'Desktop draft · Local server':'Desktop draft · '+new URL(info.server).hostname;
    const note=document.createElement('small');note.textContent='Access clears when you close the app.';
    const change=document.createElement('a');change.href='/index.html';change.textContent='Connection';
    const disconnect=document.createElement('button');disconnect.type='button';disconnect.textContent='Disconnect';
    disconnect.addEventListener('click',async()=>{await invoke('disconnect');sessionStorage.clear();location.replace('/index.html');});
    bar.append(text,note,change,disconnect);document.body.prepend(bar);
    const css=document.createElement('link');css.rel='stylesheet';css.href='/client.css';document.head.append(css);
    new MutationObserver(()=>{
      for(const [image,state]of photos)if(!image.isConnected){if(state.url)URL.revokeObjectURL(state.url);photos.delete(image);}
    }).observe(document.body,{childList:true,subtree:true});
  });
})();
