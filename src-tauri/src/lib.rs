use futures_util::StreamExt;
use reqwest::Client;
use rusqlite::{params,Connection};
use serde::{Serialize,Deserialize};
use serde_json::{json,Value};
use std::{fs::{self,File},io::{Read,Write},net::TcpListener,path::{Path,PathBuf},process::{Child,Command,Stdio},sync::{Mutex,atomic::{AtomicBool,Ordering},Arc},time::{Duration,Instant,SystemTime,UNIX_EPOCH}};
use tauri::{AppHandle,Emitter,Manager,State};
use uuid::Uuid;

#[derive(Serialize,Deserialize,Clone)]
#[serde(rename_all="camelCase")]
struct Model { id:String,name:String,path:String,bytes:i64,added_at:i64 }
#[derive(Serialize,Deserialize,Clone)]
#[serde(rename_all="camelCase")]
struct Conversation { id:String,title:String,model_id:String,updated_at:i64 }
#[derive(Serialize,Deserialize,Clone)]
#[serde(rename_all="camelCase")]
struct Message { id:String,conversation_id:String,role:String,content:String,thinking:String,status:String,created_at:i64,stats_json:String }
#[derive(Serialize,Deserialize)]
struct Turn { role:String,content:String }
struct Server { child:Child,port:u16,key:String,model_id:String,context:u32,threads:u32 }
struct AppState { home:PathBuf,db:Mutex<Connection>,server:Mutex<Option<Server>>,stop:Mutex<Option<Arc<AtomicBool>>>,client:Client }
fn error(e:impl ToString)->String{e.to_string()}
fn now()->i64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64 }
fn init_db(db:&Connection)->rusqlite::Result<()>{
    db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;
    CREATE TABLE IF NOT EXISTS models(id TEXT PRIMARY KEY,name TEXT NOT NULL,path TEXT NOT NULL,bytes INTEGER NOT NULL,added_at INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS conversations(id TEXT PRIMARY KEY,title TEXT NOT NULL,model_id TEXT NOT NULL,updated_at INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS messages(id TEXT PRIMARY KEY,conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,role TEXT NOT NULL,content TEXT NOT NULL,thinking TEXT NOT NULL DEFAULT '',status TEXT NOT NULL DEFAULT 'COMPLETED',created_at INTEGER NOT NULL,stats_json TEXT NOT NULL DEFAULT '{}');
    CREATE INDEX IF NOT EXISTS messages_conv ON messages(conversation_id,created_at);")
}
fn runtime(app:&AppHandle)->Result<PathBuf,String>{
    let resource=app.path().resource_dir().map_err(error)?;
    for dir in [resource.join("runtime"),PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime")] {
        if dir.join("llama-server.exe").is_file(){return Ok(dir)}
    }
    Err("Binary llama-server.exe tidak tersedia. Gunakan installer dari GitHub Actions.".into())
}
fn all_models(db:&Connection)->Result<Vec<Model>,String>{
    let mut stmt=db.prepare("SELECT id,name,path,bytes,added_at FROM models ORDER BY added_at DESC").map_err(error)?;
    let rows=stmt.query_map([],|r|Ok(Model{id:r.get(0)?,name:r.get(1)?,path:r.get(2)?,bytes:r.get(3)?,added_at:r.get(4)?})).map_err(error)?.collect::<Result<_,_>>().map_err(error);rows
}
fn get_model(db:&Connection,id:&str)->Result<Model,String>{
    db.query_row("SELECT id,name,path,bytes,added_at FROM models WHERE id=?",[id],|r|Ok(Model{id:r.get(0)?,name:r.get(1)?,path:r.get(2)?,bytes:r.get(3)?,added_at:r.get(4)?})).map_err(error)
}
fn all_conversations(db:&Connection)->Result<Vec<Conversation>,String>{
    let mut stmt=db.prepare("SELECT id,title,model_id,updated_at FROM conversations ORDER BY updated_at DESC").map_err(error)?;
    let rows=stmt.query_map([],|r|Ok(Conversation{id:r.get(0)?,title:r.get(1)?,model_id:r.get(2)?,updated_at:r.get(3)?})).map_err(error)?.collect::<Result<_,_>>().map_err(error);rows
}
#[tauri::command]
fn bootstrap(app:AppHandle,state:State<'_,AppState>)->Result<Value,String>{
    let db=state.db.lock().map_err(error)?;
    Ok(json!({"models":all_models(&db)?,"conversations":all_conversations(&db)?,"runtimeReady":runtime(&app).is_ok()}))
}
#[tauri::command]
async fn import_model(app:AppHandle,state:State<'_,AppState>)->Result<Option<Model>,String>{
    let chosen=tauri::async_runtime::spawn_blocking(||rfd::FileDialog::new().add_filter("GGUF",&["gguf"]).pick_file()).await.map_err(error)?;
    let Some(source)=chosen else{return Ok(None)};
    let home=state.home.clone();
    let name=source.file_stem().unwrap_or_default().to_string_lossy().replace('_'," ");
    let id=Uuid::new_v4().to_string();
    let destdir=home.join("models").join(&id);
    fs::create_dir_all(&destdir).map_err(error)?;
    let final_path=destdir.join("model.gguf");
    let event_app=app.clone(); let event_id=id.clone();
    let size=tauri::async_runtime::spawn_blocking(move ||->Result<i64,String>{
        let mut input=File::open(&source).map_err(error)?;
        let mut magic=[0u8;4]; input.read_exact(&mut magic).map_err(error)?;
        if &magic!=b"GGUF"{return Err("File ini bukan model GGUF yang valid".into())}
        let total=input.metadata().map_err(error)?.len();
        if total<1024 {return Err("File GGUF terlalu kecil".into())}
        let part=final_path.with_extension("part");
        let copy=(||->Result<i64,String>{
            let mut output=File::create(&part).map_err(error)?;
            output.write_all(&magic).map_err(error)?;
            let mut copied=4u64; let mut buf=vec![0u8;1024*1024];
            let mut last=Instant::now();
            loop {
                let n=input.read(&mut buf).map_err(error)?; if n==0{break}
                output.write_all(&buf[..n]).map_err(error)?;copied+=n as u64;
                if last.elapsed()>=Duration::from_millis(170){
                    let _=event_app.emit("model:progress",json!({"id":event_id,"copied":copied,"total":total}));
                    last=Instant::now();
                }
            }
            output.sync_all().map_err(error)?;
            if copied!=total {return Err("Salinan model tidak lengkap".into())}
            fs::rename(&part,&final_path).map_err(error)?;
            let _=event_app.emit("model:progress",json!({"id":event_id,"copied":total,"total":total}));
            Ok(total as i64)
        })();
        if copy.is_err(){let _=fs::remove_file(&part);}
        copy
    }).await.map_err(error)??;
    let saved=Model{id:id.clone(),name,path:destdir.join("model.gguf").to_string_lossy().into_owned(),bytes:size,added_at:now()};
    state.db.lock().map_err(error)?.execute("INSERT INTO models VALUES(?,?,?,?,?)",params![saved.id,saved.name,saved.path,saved.bytes,saved.added_at]).map_err(error)?;
    Ok(Some(saved))
}
fn stop_runtime(state:&AppState)->Result<(),String>{
    if let Ok(mut guard)=state.stop.lock(){if let Some(token)=guard.take(){token.store(true,Ordering::Relaxed)}}
    if let Some(mut s)=state.server.lock().map_err(error)?.take(){let _=s.child.kill();let _=s.child.wait();}
    Ok(())
}
#[tauri::command]
fn unload_model(state:State<'_,AppState>)->Result<(),String>{stop_runtime(&state)}
#[tauri::command]
fn delete_model(state:State<'_,AppState>,id:String)->Result<(),String>{
    if state.server.lock().map_err(error)?.as_ref().is_some_and(|s|s.model_id==id){return Err("Lepaskan model aktif sebelum menghapusnya".into())}
    let db=state.db.lock().map_err(error)?;
    let model=get_model(&db,&id)?;
    db.execute("DELETE FROM models WHERE id=?",[&id]).map_err(error)?;
    if let Some(parent)=Path::new(&model.path).parent(){if parent.starts_with(state.home.join("models")){let _=fs::remove_dir_all(parent);}}
    Ok(())
}
#[tauri::command]
async fn load_model(app:AppHandle,state:State<'_,AppState>,id:String,context:u32,threads:u32)->Result<(),String>{
    let context=context.clamp(1024,32768);let threads=threads.clamp(1,64);
    let model={let db=state.db.lock().map_err(error)?;get_model(&db,&id)?};
    if !Path::new(&model.path).is_file(){return Err("File model tidak ditemukan".into())}
    {
        let mut lock=state.server.lock().map_err(error)?;
        if let Some(ref mut server)=*lock {
            if server.model_id==id&&server.context==context&&server.threads==threads&&server.child.try_wait().map_err(error)?.is_none(){return Ok(())}
        }
    }
    stop_runtime(&state)?;
    let _=app.emit("model:phase",json!({"phase":"loading","name":model.name}));
    let dir=runtime(&app)?;
    let port=TcpListener::bind("127.0.0.1:0").map_err(error)?.local_addr().map_err(error)?.port();
    let key=Uuid::new_v4().to_string();
    let logs=state.home.join("logs");fs::create_dir_all(&logs).map_err(error)?;
    let logfile=logs.join("llama-server.log");let log=File::create(&logfile).map_err(error)?;
    let mut process=Command::new(dir.join("llama-server.exe"));
    process.current_dir(&dir).args(["--model",&model.path,"--host","127.0.0.1","--port",&port.to_string(),"--ctx-size",&context.to_string(),"--threads",&threads.to_string(),"--n-gpu-layers","0","--api-key",&key,"--no-webui","--parallel","1"])
      .stdin(Stdio::null()).stdout(Stdio::from(log.try_clone().map_err(error)?)).stderr(Stdio::from(log));
    #[cfg(windows)] {
        use std::os::windows::process::CommandExt;
        process.creation_flags(0x08000000);
    }
    let mut path=std::ffi::OsString::from(dir.as_os_str());path.push(";");path.push(std::env::var_os("PATH").unwrap_or_default());process.env("PATH",path);
    let child=process.spawn().map_err(|e|format!("llama-server gagal dijalankan: {e}"))?;
    *state.server.lock().map_err(error)?=Some(Server{child,port,key:key.clone(),model_id:id,context,threads});
    let check=async{
        for _ in 0..240 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            {
                let mut lock=state.server.lock().map_err(error)?;
                let Some(server)=lock.as_mut() else{return Err("Pemilihan model dibatalkan".into())};
                if let Some(code)=server.child.try_wait().map_err(error)? {return Err(format!("Runtime berhenti ({code}). Log: {}",logfile.display()))}
            }
            if let Ok(reply)=state.client.get(format!("http://127.0.0.1:{port}/health")).bearer_auth(&key).timeout(Duration::from_secs(2)).send().await{
                if reply.status().is_success(){return Ok(())}
            }
        }
        Err("Model belum siap. Coba ulangi dengan konteks lebih kecil.".into())
    }.await;
    if check.is_err(){let _=stop_runtime(&state);}
    let _=app.emit("model:phase",json!({"phase":if check.is_ok(){"ready"}else{"failed"},"name":model.name}));
    check
}
#[tauri::command]
fn new_conversation(state:State<'_,AppState>,model_id:String)->Result<Conversation,String>{
    let c=Conversation{id:Uuid::new_v4().to_string(),title:"Percakapan baru".into(),model_id,updated_at:now()};
    state.db.lock().map_err(error)?.execute("INSERT INTO conversations VALUES(?,?,?,?)",params![c.id,c.title,c.model_id,c.updated_at]).map_err(error)?;
    Ok(c)
}
#[tauri::command]
fn rename_conversation(state:State<'_,AppState>,id:String,title:String)->Result<(),String>{
    state.db.lock().map_err(error)?.execute("UPDATE conversations SET title=?,updated_at=? WHERE id=?",params![title.chars().take(100).collect::<String>(),now(),id]).map_err(error)?;
    Ok(())
}
#[tauri::command]
fn delete_conversation(state:State<'_,AppState>,id:String)->Result<(),String>{
    state.db.lock().map_err(error)?.execute("DELETE FROM conversations WHERE id=?",[id]).map_err(error)?;Ok(())
}
#[tauri::command]
fn get_messages(state:State<'_,AppState>,conversation_id:String)->Result<Vec<Message>,String>{
    let db=state.db.lock().map_err(error)?;
    let mut stmt=db.prepare("SELECT id,conversation_id,role,content,thinking,status,created_at,stats_json FROM messages WHERE conversation_id=? ORDER BY created_at,id").map_err(error)?;
    let rows=stmt.query_map([conversation_id],|r|Ok(Message{id:r.get(0)?,conversation_id:r.get(1)?,role:r.get(2)?,content:r.get(3)?,thinking:r.get(4)?,status:r.get(5)?,created_at:r.get(6)?,stats_json:r.get(7)?})).map_err(error)?.collect::<Result<_,_>>().map_err(error);rows
}
#[tauri::command]
fn save_message(state:State<'_,AppState>,message:Message)->Result<(),String>{
    if !["user","assistant"].contains(&message.role.as_str()){return Err("Peran pesan salah".into())}
    let db=state.db.lock().map_err(error)?;
    db.execute("INSERT INTO messages VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET content=excluded.content,thinking=excluded.thinking,status=excluded.status,stats_json=excluded.stats_json",params![message.id,message.conversation_id,message.role,message.content,message.thinking,message.status,message.created_at,message.stats_json]).map_err(error)?;
    db.execute("UPDATE conversations SET updated_at=? WHERE id=?",params![now(),message.conversation_id]).map_err(error)?;
    Ok(())
}
#[tauri::command]
fn stop_generation(state:State<'_,AppState>)->Result<(),String>{
    if let Some(ref token)=*state.stop.lock().map_err(error)?{token.store(true,Ordering::Relaxed)}
    Ok(())
}
fn active(state:&AppState)->Result<(String,String),String>{
    let lock=state.server.lock().map_err(error)?;
    let Some(ref s)=*lock else{return Err("Muat model terlebih dahulu".into())};
    Ok((format!("http://127.0.0.1:{}/v1/chat/completions",s.port),s.key.clone()))
}
#[tauri::command]
async fn generate(app:AppHandle,state:State<'_,AppState>,request_id:String,messages:Vec<Turn>,temperature:f32,max_tokens:u32)->Result<(),String>{
    let (url,key)=active(&state)?;
    let stop=Arc::new(AtomicBool::new(false));
    {let mut guard=state.stop.lock().map_err(error)?;if guard.is_some(){return Err("Masih ada generasi berjalan".into())}*guard=Some(stop.clone());}
    let timer=Instant::now();
    let input=messages.into_iter().map(|m|json!({"role":m.role,"content":m.content})).collect::<Vec<_>>();
    let result=state.client.post(url).bearer_auth(key).json(&json!({"model":"local","messages":input,"stream":true,"stream_options":{"include_usage":true},"temperature":temperature.clamp(0.0,1.5),"max_tokens":max_tokens.clamp(64,8192)})).send().await;
    let mut error_text=String::new();let mut count=0u64;let mut status="COMPLETED";let mut content=String::new();
    match result{
        Ok(mut response)=>{
            if !response.status().is_success(){
                status="FAILED";error_text=response.text().await.unwrap_or_else(|_|"Server gagal memproses permintaan".into());
            }else{
                let mut bytes=Vec::<u8>::new();
                'stream: loop {
                    if stop.load(Ordering::Relaxed){status="STOPPED";break;}
                    match response.chunk().await {
                        Ok(Some(chunk))=>{
                            bytes.extend_from_slice(&chunk);
                            while let Some(pos)=bytes.windows(2).position(|x|x==b"\n\n"){
                                let event=bytes.drain(..pos+2).collect::<Vec<u8>>();
                                for line in String::from_utf8_lossy(&event).lines(){
                                    let Some(data)=line.strip_prefix("data: ") else{continue};
                                    if data=="[DONE]"{break}
                                    if let Ok(v)=serde_json::from_str::<Value>(data){
                                        count=v.pointer("/usage/completion_tokens").and_then(Value::as_u64).unwrap_or(count);
                                        if let Some(delta)=v.pointer("/choices/0/delta"){
                                            let part=delta.get("content").and_then(Value::as_str).unwrap_or("");
                                            let thought=delta.get("reasoning_content").and_then(Value::as_str).unwrap_or("");
                                            if !part.is_empty() || !thought.is_empty(){
                                                content.push_str(part);
                                                let _=app.emit("chat:delta",json!({"id":request_id,"text":part,"thinking":thought}));
                                            }
                                        }
                                        if v.pointer("/choices/0/finish_reason").and_then(Value::as_str)==Some("length"){status="LIMIT_REACHED";}
                                    }
                                }
                            }
                        }
                        Ok(None)=>break 'stream,
                        Err(e)=>{status="FAILED";error_text=e.to_string();break 'stream;}
                    }
                }
            }
        }
        Err(e)=>{status="FAILED";error_text=e.to_string();}
    }
    if stop.load(Ordering::Relaxed){status="STOPPED"}
    let ms=timer.elapsed().as_millis() as u64;
    let tps=if count>0&&ms>0{count as f64*1000.0/ms as f64}else{0.0};
    let _=app.emit("chat:done",json!({"id":request_id,"status":status,"elapsedMs":ms,"tokens":count,"tps":tps,"error":error_text,"textBytes":content.len()}));
    *state.stop.lock().map_err(error)?=None;
    Ok(())
}
pub fn run(){
    tauri::Builder::default()
     .setup(|app|{
        let root=app.path().app_data_dir()?;
        fs::create_dir_all(root.join("models"))?;
        let db=Connection::open(root.join("nalaro.db"))?;init_db(&db)?;
        let client=Client::builder().no_proxy().timeout(Duration::from_secs(900)).build()?;
        app.manage(AppState{home:root,db:Mutex::new(db),server:Mutex::new(None),stop:Mutex::new(None),client});Ok(())
     })
     .invoke_handler(tauri::generate_handler![bootstrap,import_model,delete_model,unload_model,load_model,new_conversation,rename_conversation,delete_conversation,get_messages,save_message,stop_generation,generate])
     .build(tauri::generate_context!()).expect("Nalaro AI failed to initialize")
     .run(|app,event|{if let tauri::RunEvent::Exit=event{let _=stop_runtime(&app.state::<AppState>());}});
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn sqlite_cleanup(){
        let db=Connection::open_in_memory().unwrap();init_db(&db).unwrap();
        db.execute("INSERT INTO conversations VALUES('a','chat','',1)",[]).unwrap();
        db.execute("INSERT INTO messages(id,conversation_id,role,content,created_at) VALUES('m','a','user','hi',1)",[]).unwrap();
        db.execute("DELETE FROM conversations WHERE id='a'",[]).unwrap();
        let num:i64=db.query_row("SELECT COUNT(*) FROM messages",[],|r|r.get(0)).unwrap();assert_eq!(num,0);
    }
}
