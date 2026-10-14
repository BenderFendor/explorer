//! Durable, bounded undo/redo journal for moves, copies and newly created files.
//! Undo of newly created files stashes rather than deletes their bytes.
use serde::{Serialize,Deserialize};
use std::{fs,path::{Path,PathBuf},sync::{Mutex,OnceLock,atomic::{AtomicU64,Ordering}},time::{SystemTime,UNIX_EPOCH}};

#[derive(Clone,Serialize,Deserialize,Debug)]
enum Change { Move{from:PathBuf,to:PathBuf}, Add{path:PathBuf,stash:PathBuf} }
#[derive(Default,Serialize,Deserialize)]
struct Journal {undo:Vec<Change>,redo:Vec<Change>}
static JOURNAL:OnceLock<Mutex<Journal>>=OnceLock::new();
static SERIAL:AtomicU64=AtomicU64::new(0);
fn base()->PathBuf {
    let dir=std::env::var_os("EXPLORER_STATE_DIR").map(PathBuf::from)
        .or_else(||directories::BaseDirs::new().map(|d|d.state_dir().unwrap_or(d.data_local_dir()).to_path_buf()))
        .unwrap_or_else(||crate::filesystem::home().join(".local/state"));
    dir.join("explorer")
}
fn journal()->&'static Mutex<Journal>{
    JOURNAL.get_or_init(||{
        let state=fs::read(base().join("undo.json")).ok()
            .and_then(|data|serde_json::from_slice(&data).ok()).unwrap_or_default();
        Mutex::new(state)
    })
}
fn persist(j:&Journal)->Result<(),String>{
    fs::create_dir_all(base()).map_err(|e|e.to_string())?;
    let tmp=base().join("undo.tmp");
    fs::write(&tmp,serde_json::to_vec(j).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    fs::rename(tmp,base().join("undo.json")).map_err(|e|e.to_string())
}
fn stash_name(path:&Path)->PathBuf {
    let tick=SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let serial=SERIAL.fetch_add(1,Ordering::Relaxed);
    let ext=path.extension().map(|x|x.to_string_lossy().into_owned()).unwrap_or_default();
    base().join("undo-stash").join(format!("{}-{}-{}.{}",std::process::id(),tick,serial,ext))
}
fn record(change:Change) {
    if let Ok(mut journal)=journal().lock() {
        journal.undo.push(change);
        if journal.undo.len()>100 {journal.undo.remove(0);}
        journal.redo.clear();
        let _=persist(&journal);
    }
}
pub fn record_move(from:&Path,to:&Path){record(Change::Move{from:from.into(),to:to.into()});}
pub fn record_add(path:&Path){record(Change::Add{path:path.into(),stash:stash_name(path)});}

fn apply(change:&Change,undo:bool)->Result<(),String>{
    match change {
        Change::Move{from,to} => if undo{crate::operations::move_exact(to,from)}else{crate::operations::move_exact(from,to)},
        Change::Add{path,stash} => {
            if undo {
                if let Some(parent)=stash.parent(){fs::create_dir_all(parent).map_err(|e|e.to_string())?;}
                crate::operations::move_exact(path,stash)
            }else {crate::operations::move_exact(stash,path)}
        }
    }
}
pub fn undo()->Result<String,String>{execute(true)}
pub fn redo()->Result<String,String>{execute(false)}
fn execute(reverse:bool)->Result<String,String>{
    let mut j=journal().lock().map_err(|e|e.to_string())?;
    let change=if reverse{j.undo.last()}else{j.redo.last()}.cloned().ok_or_else(||"Nothing to undo/redo".to_string())?;
    apply(&change,reverse)?;
    if reverse {j.undo.pop();j.redo.push(change);}
    else{j.redo.pop();j.undo.push(change);}
    persist(&j)?;
    Ok(if reverse{"Undone".into()}else{"Redone".into()})
}
