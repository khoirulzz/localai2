'use strict';
const $=q=>document.querySelector(q),call=(name,args={})=>window.__TAURI__.core.invoke(name,args),on=(event,fn)=>window.__TAURI__.event.listen(event,e=>fn(e.payload));
const defaults={temperature:.6,context:4096,threads:4,tokens:2048,thinking:'OFF',instructions:''};
const state={models:[],conversations:[],messages:[],current:null,modelId:localStorage.getItem('nalaro:model')||'',preferences:{...defaults,...JSON.parse(localStorage.getItem('nalaro:prefs')||'{}')},busy:false,loading:false,request:'',text:'',reasoning:'',assistant:null,follow:true,ready:false};
const uid=()=>crypto.randomUUID(),now=()=>Date.now(),model=()=>state.models.find(m=>m.id===state.modelId);
const escapeHTML=t=>String(t??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const formatBytes=n=>(n/1024/1024).toFixed(n>1024**3?0:0)+' MB';
function markdown(text){return escapeHTML(text).replace(/\*\*([^\n*]+)\*\*/g,'<b>$1</b>').replace(/\n/g,'<br>')}
function splitOutput(){
  let text=state.text,thought=state.reasoning;
  const i=text.indexOf('<think>');
  if(i>=0){const j=text.indexOf('</think>',i+7);thought+=(j<0?text.slice(i+7):text.slice(i+7,j));text=text.slice(0,i)+(j<0?'':text.slice(j+8))}
  return {text,thought};
}
function toast(t){const e=$('#toast');e.textContent=t;e.hidden=false;clearTimeout(toast.timer);toast.timer=setTimeout(()=>e.hidden=true,4000)}
function notice(t){const e=$('#notice');e.textContent=t;e.hidden=false;clearTimeout(notice.timer);notice.timer=setTimeout(()=>e.hidden=true,8000)}
function status(t,cls=''){const e=$('#status');e.className='status '+cls;$('#statusText').textContent=t}
function scrollEnd(){requestAnimationFrame(()=>{$('#scroll').scrollTop=$('#scroll').scrollHeight})}
function message(role,content,st='COMPLETED'){return {id:uid(),conversationId:state.current.id,role,content,thinking:'',status:st,createdAt:now(),statsJson:'{}'}}
function renderMessages(){
  const view=$('#messages');view.innerHTML='';$('#welcome').hidden=state.messages.length>0;
  for(const m of state.messages){
    const row=document.createElement('article');row.className='message '+m.role;row.dataset.id=m.id;
    if(m.role==='user'){row.textContent=m.content;view.append(row);continue}
    const think=document.createElement('details');think.className='thought';think.hidden=!m.thinking;think.innerHTML='<summary>Proses berpikir</summary><pre></pre>';think.querySelector('pre').textContent=m.thinking;
    const content=document.createElement('div');content.className='msg-content';content.innerHTML=m.content?markdown(m.content):'<span class="dots"><i></i><i></i><i></i></span>';
    const tools=document.createElement('div');tools.className='msg-tools';const copy=document.createElement('button');copy.className='copy';copy.textContent='Salin';copy.onclick=()=>navigator.clipboard.writeText(m.content).then(()=>toast('Disalin')).catch(()=>{});
    const info=document.createElement('span');info.textContent=statistics(m);tools.append(copy,info);row.append(think,content,tools);view.append(row);
  }
  if(state.follow)scrollEnd();
}
function statistics(m){
  let p={};try{p=JSON.parse(m.statsJson||'{}')}catch{}
  return m.status==='STREAMING'?'Menulis…':m.status==='FAILED'?'Gagal':m.status==='STOPPED'?'Dihentikan':[p.tokens?p.tokens+' token':'',p.tps?p.tps.toFixed(1)+' t/s':'',p.elapsedMs?(p.elapsedMs/1000).toFixed(1)+' dtk':''].filter(Boolean).join(' · ');
}
function refreshOutput(){
  const m=state.assistant;if(!m)return;
  const t=splitOutput();m.content=t.text;m.thinking=t.thought;
  const el=document.querySelector('[data-id="'+m.id+'"]');if(!el)return;
  el.querySelector('.thought').hidden=!m.thinking;el.querySelector('.thought pre').textContent=m.thinking;
  el.querySelector('.msg-content').innerHTML=m.content?markdown(m.content):'<span class="dots"><i></i><i></i><i></i></span>';
  el.querySelector('.msg-tools span').textContent=statistics(m);if(state.follow)scrollEnd();
}
function sidebar(){
  const root=$('#conversations');root.innerHTML='';
  for(const c of [...state.conversations].sort((a,b)=>b.updatedAt-a.updatedAt)){
    const b=document.createElement('button');b.className='conversation-item'+(state.current?.id===c.id?' active':'');
    const name=document.createElement('span');name.textContent=c.title;const del=document.createElement('i');del.textContent='×';
    del.onclick=async e=>{e.stopPropagation();if(!confirm('Hapus percakapan?'))return;try{await call('delete_conversation',{id:c.id});state.conversations=state.conversations.filter(x=>x.id!==c.id);if(state.current?.id===c.id){state.current=null;state.messages=[];renderMessages()}sidebar()}catch(e){toast(String(e))}};
    b.onclick=()=>openConversation(c.id);b.append(name,del);root.append(b);
  }
}
async function openConversation(id){if(state.busy)return toast('Hentikan jawaban dahulu');try{state.current=state.conversations.find(c=>c.id===id);state.messages=await call('get_messages',{conversationId:id});sidebar();renderMessages()}catch(e){toast(String(e))}}
async function newConversation(){if(state.busy)return toast('Hentikan jawaban dahulu');try{const c=await call('new_conversation',{modelId:state.modelId||''});state.conversations.unshift(c);state.current=c;state.messages=[];sidebar();renderMessages();$('#prompt').focus()}catch(e){toast(String(e))}}
function closeMenus(){for(const menu of document.querySelectorAll('.floating'))menu.hidden=true}
function chooseMenu(id,anchor){closeMenus();const menu=$(id);menu.hidden=false;const a=anchor.getBoundingClientRect();menu.style.left=Math.max(8,Math.min(innerWidth-menu.offsetWidth-8,a.left))+'px';menu.style.top=Math.max(8,a.top-menu.offsetHeight-8>8?a.top-menu.offsetHeight-8:a.bottom+8)+'px'}
function refresh(){
  const m=model();$('#pick').firstChild.textContent=m?m.name+' ':'Pilih model ';
  $('#send').textContent=state.busy?'■':'↑';$('#send').disabled=state.loading||(!state.busy&&!$('#prompt').value.trim());$('#send').classList.toggle('stopping',state.busy);
  $('#thinkingText').textContent=({OFF:'Cepat',STANDARD:'Pikir',DEEP:'Pikir lebih'})[state.preferences.thinking];renderMenu();
}
function renderMenu(){const root=$('#modelMenu');root.innerHTML='';
  for(const m of state.models){const b=document.createElement('button');b.innerHTML='<b>'+escapeHTML(m.name)+'</b><small>'+formatBytes(m.bytes)+(state.modelId===m.id?' · Dipilih':'')+'</small>';b.onclick=()=>{state.modelId=m.id;state.ready=false;localStorage.setItem('nalaro:model',m.id);closeMenus();refresh()};root.append(b)}
  const manage=document.createElement('button');manage.textContent='Kelola model';manage.onclick=()=>show('modelDialog');root.append(manage);
}
function show(id){closeMenus();$('#overlay').hidden=false;$('#modelDialog').hidden=id!=='modelDialog';$('#settingsDialog').hidden=id!=='settingsDialog';if(id==='modelDialog')renderModels();else fillSettings()}
function hide(){$('#overlay').hidden=true;$('#modelDialog').hidden=true;$('#settingsDialog').hidden=true}
function renderModels(){const root=$('#modelList');root.innerHTML='';
  if(!state.models.length)root.innerHTML='<p class="muted">Belum ada model. Impor GGUF dari komputer.</p>';
  for(const m of state.models){
    const row=document.createElement('div');row.className='model-row';const info=document.createElement('div');info.style.flex='1';info.innerHTML='<b>'+escapeHTML(m.name)+'</b><small>'+formatBytes(m.bytes)+'</small>';
    const actions=document.createElement('div');actions.className='model-actions';
    const use=document.createElement('button');use.textContent=m.id===state.modelId?'Dipilih':'Pakai';use.onclick=()=>{state.modelId=m.id;localStorage.setItem('nalaro:model',m.id);state.ready=false;renderModels();refresh()};
    const remove=document.createElement('button');remove.textContent='Hapus';remove.className='danger';remove.onclick=async()=>{if(!confirm('Hapus model GGUF ini?'))return;try{if(state.modelId===m.id){await call('unload_model');state.modelId='';state.ready=false;localStorage.removeItem('nalaro:model')}await call('delete_model',{id:m.id});state.models=state.models.filter(x=>x.id!==m.id);renderModels();refresh()}catch(e){toast(String(e))}};
    actions.append(use,remove);row.append(info,actions);root.append(row);
  }
}
async function importModel(){$('#progress').hidden=false;$('#progressText').textContent='Pilih file…';try{const m=await call('import_model');if(m){state.models.unshift(m);state.modelId=m.id;state.ready=false;localStorage.setItem('nalaro:model',m.id);renderModels();refresh();toast('Model diimpor')}}catch(e){toast(String(e))}finally{$('#progress').hidden=true}}
async function send(){
  if(state.busy){await call('stop_generation');return}
  const text=$('#prompt').value.trim();if(!text)return;if(!model())return show('modelDialog');if(!state.current)await newConversation();
  $('#prompt').value='';state.follow=true;state.loading=true;refresh();
  const user=message('user',text);state.messages.push(user);renderMessages();await call('save_message',{message:user});
  if(state.current.title==='Percakapan baru'){const title=text.replace(/\s+/g,' ').slice(0,60);state.current.title=title;state.current.updatedAt=now();sidebar();call('rename_conversation',{id:state.current.id,title}).catch(()=>{})}
  try{
    status('Memuat model…','loading');
    await call('load_model',{id:state.modelId,context:Number(state.preferences.context),threads:Number(state.preferences.threads)});
    state.loading=false;state.busy=true;state.ready=true;state.request=uid();state.text='';state.reasoning='';state.assistant=message('assistant','','STREAMING');state.messages.push(state.assistant);renderMessages();refresh();
    const history=state.messages.slice(0,-2).filter(x=>x.status!=='FAILED').slice(-12).map(x=>({role:x.role,content:x.content}));
    const instructions='Kamu Nalaro AI, asisten lokal. Jawab secara ringkas, akurat, dalam bahasa pengguna. '+state.preferences.instructions+' Mode berpikir: '+state.preferences.thinking+'.';
    const turns=[{role:'system',content:instructions},...history,{role:'user',content:text}];
    call('generate',{requestId:state.request,messages:turns,temperature:Number(state.preferences.temperature),maxTokens:Number(state.preferences.tokens)}).catch(e=>finish({id:state.request,status:'FAILED',error:String(e)}));
  }catch(e){state.loading=false;state.busy=false;status('Gagal memuat');notice(String(e));refresh()}
}
async function finish(payload){
  if(!state.busy||payload.id!==state.request)return;
  refreshOutput();const m=state.assistant;state.busy=false;state.loading=false;
  if(m){m.status=payload.status||'COMPLETED';m.statsJson=JSON.stringify({elapsedMs:payload.elapsedMs||0,tokens:payload.tokens||0,tps:payload.tps||0});try{await call('save_message',{message:m})}catch(e){toast(String(e))}refreshOutput()}
  if(payload.error)notice('Kesalahan: '+payload.error);state.assistant=null;state.request='';status('Siap','ready');refresh();
}
function fillSettings(){const p=state.preferences;$('#temp').value=p.temperature;$('#context').value=p.context;$('#threads').value=p.threads;$('#tokens').value=p.tokens;$('#instructions').value=p.instructions;updateRange()}
function updateRange(){$('#tempValue').textContent=Number($('#temp').value).toFixed(2);$('#threadValue').textContent=$('#threads').value}
async function boot(){
  try{
    await on('model:progress',e=>{if(!e.total)return;const pct=Math.min(100,100*e.copied/e.total);$('#progressFill').style.width=pct+'%';$('#progressText').textContent=pct.toFixed(0)+'%'});
    await on('model:phase',e=>status(e.phase==='loading'?'Memuat '+e.name+'…':'Siap',e.phase==='ready'?'ready':'loading'));
    await on('chat:delta',e=>{if(e.id!==state.request)return;state.text+=e.text||'';state.reasoning+=e.thinking||'';clearTimeout(state.timer);state.timer=setTimeout(refreshOutput,70)});
    await on('chat:done',finish);
    const data=await call('bootstrap');state.models=data.models||[];state.conversations=data.conversations||[];if(!data.runtimeReady)notice('Runtime llama-server belum tersedia. Gunakan installer GitHub Actions.');
    if(!model()&&state.models[0]){state.modelId=state.models[0].id;localStorage.setItem('nalaro:model',state.modelId)}
    if(state.conversations[0])await openConversation(state.conversations[0].id);else renderMessages();
    sidebar();refresh();status(state.models.length?'Model tersedia':'Belum ada model');
  }catch(e){notice('Gagal membuka Nalaro AI: '+e)}
}
$('#new').onclick=newConversation;$('#models').onclick=()=>show('modelDialog');$('#settings').onclick=()=>show('settingsDialog');$('#firstImport').onclick=()=>show('modelDialog');$('#import').onclick=importModel;
$('#collapse').onclick=()=>document.body.classList.add('sidebar-hidden');$('#expand').onclick=()=>document.body.classList.remove('sidebar-hidden');
$('#pick').onclick=e=>chooseMenu('#modelMenu',e.currentTarget);$('#chooseModel').onclick=e=>chooseMenu('#modelMenu',e.currentTarget);$('#thinking').onclick=e=>chooseMenu('#thinkMenu',e.currentTarget);
for(const b of document.querySelectorAll('[data-mode]'))b.onclick=()=>{state.preferences.thinking=b.dataset.mode;localStorage.setItem('nalaro:prefs',JSON.stringify(state.preferences));closeMenus();refresh()};
$('#send').onclick=send;$('#overlay').onclick=hide;for(const b of document.querySelectorAll('.close'))b.onclick=hide;
$('#temp').oninput=updateRange;$('#threads').oninput=updateRange;$('#savePrefs').onclick=()=>{state.preferences={...state.preferences,temperature:Number($('#temp').value),context:Number($('#context').value),threads:Number($('#threads').value),tokens:Number($('#tokens').value),instructions:$('#instructions').value};localStorage.setItem('nalaro:prefs',JSON.stringify(state.preferences));state.ready=false;hide();toast('Pengaturan disimpan')};
$('#prompt').onkeydown=e=>{if(e.key==='Enter'&&!e.shiftKey){e.preventDefault();send()}};$('#prompt').oninput=()=>{const p=$('#prompt');p.style.height='auto';p.style.height=Math.min(150,p.scrollHeight)+'px';refresh()};
$('#scroll').onscroll=()=>{const e=$('#scroll');state.follow=e.scrollHeight-e.scrollTop-e.clientHeight<140};
document.addEventListener('click',e=>{if(!e.target.closest('.floating,.model-picker,.chip'))closeMenus()});
document.addEventListener('keydown',e=>{if(e.ctrlKey&&e.key.toLowerCase()==='k'){e.preventDefault();newConversation()}if(e.key==='Escape'){hide();closeMenus()}});
boot();
