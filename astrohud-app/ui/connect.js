addEventListener('DOMContentLoaded',async()=>{
  const info=await window.memoryLaneReady;
  const form=document.querySelector('#connect'),server=document.querySelector('#server'),status=document.querySelector('#connection-status');
  server.value=info.server;
  const showDemo=()=>{try{document.querySelector('#demo-card').hidden=!['localhost','127.0.0.1','[::1]'].includes(new URL(server.value).hostname);}catch(_){document.querySelector('#demo-card').hidden=true;}};
  showDemo();server.addEventListener('input',showDemo);
  document.querySelector('#demo').addEventListener('click',async()=>{
    try{
      const next=await window.memoryLaneInvoke('set_server',{server:server.value});
      if(!next.local)throw Error('Sample frames require a local development server.');
      location.href='/local-simulator.html';
    }catch(error){status.textContent=String(error);}
  });
  document.querySelector('#resume').addEventListener('click',async()=>{
    try{
      const owner=await fetch('/api/beta/owner');
      if(owner.ok){location.href='/owner.html';return;}
      const sender=await fetch('/api/beta/sender');
      if(sender.ok){location.href='/send.html';return;}
      status.textContent='Connect with your private link first.';
    }catch(_){status.textContent='Could not reach the server. Check the address and connection.';}
  });
  form.addEventListener('submit',async event=>{
    event.preventDefault();if(!form.reportValidity())return;
    const controls=[...form.querySelectorAll('button,input')];controls.forEach(c=>c.disabled=true);
    status.textContent='Connecting…';
    try{
      const link=new URL(document.querySelector('#private-link').value.trim());
      const endpoints={'/activate.html':'/api/beta/owner/session','/recover.html':'/api/beta/owner/recovery/session','/sender-activate.html':'/api/beta/sender/session'};
      const endpoint=endpoints[link.pathname],token=link.hash.slice(1);
      if(!endpoint||token.length<32||token.length>512||link.username||link.password)throw Error('Use a complete Memory Lane private link, including the part after #.');
      if(link.origin!==new URL(server.value).origin)throw Error('This link belongs to a different server. Update the Server address first.');
      const next=await window.memoryLaneInvoke('set_server',{server:server.value});window.memoryLaneServerOrigin=next.server;window.memoryLaneLocalServer=next.local;
      const response=await fetch(endpoint,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({token})});
      if(!response.ok)throw Error('This link is invalid or no longer active. Check the original link and try again.');
      document.querySelector('#private-link').value='';
      location.replace(endpoint.includes('/sender/')?'/send.html':'/owner.html');
    }catch(error){status.textContent=String(error.message||error);}
    finally{controls.forEach(c=>c.disabled=false);}
  });
});
