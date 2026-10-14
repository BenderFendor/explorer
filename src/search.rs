//! Opt-in incremental recursive search: filenames, image metadata and small text files.
//! No daemon, shell invocation or invisible filesystem scan.
use serde::{Deserialize, Serialize};
use std::{collections::{HashMap,HashSet},fs,io, path::{Path,PathBuf}, time::{SystemTime,UNIX_EPOCH}};

#[derive(Clone,Debug,Default,Serialize,Deserialize)]
pub struct Document {
    pub path:PathBuf,
    pub modified:u64,
    pub len:u64,
    pub content:String,
}

#[derive(Clone,Debug,Default,Serialize,Deserialize)]
struct Index {root:PathBuf,documents:Vec<Document>}

fn index_path(root:&Path)->PathBuf {
    let base=std::env::var_os("EXPLORER_STATE_DIR").map(PathBuf::from)
        .or_else(||directories::BaseDirs::new().map(|dirs|dirs.state_dir().unwrap_or(dirs.data_local_dir()).to_path_buf()))
        .unwrap_or_else(||crate::filesystem::home().join(".local/state"));
    base.join("explorer/index").join(format!("{:x}.json",md5::compute(root.as_os_str().as_encoded_bytes())))
}

fn read_index(root:&Path)->Index {
    fs::read(index_path(root)).ok().and_then(|bytes|serde_json::from_slice::<Index>(&bytes).ok())
        .filter(|idx|idx.root==root).unwrap_or_else(||Index{root:root.to_path_buf(),documents:Vec::new()})
}

fn stamp(time:Result<SystemTime,io::Error>)->u64 {
    time.ok().and_then(|m|m.duration_since(UNIX_EPOCH).ok()).map(|v|v.as_millis() as u64).unwrap_or(0)
}

fn summarize(path:&Path)->String {
    let mut content=String::new();
    if matches!(path.extension().and_then(|s|s.to_str()).unwrap_or("").to_lowercase().as_str(),
             "jpg"|"jpeg"|"png"|"gif"|"webp"|"avif"|"bmp"|"tif"|"tiff") {
        let info=crate::image_metadata::load(path);
        content.push_str(&format!("{} {} {} {} {} {}",info.title,info.caption,info.notes,info.creator,info.source,info.tags.join(" ")));
    } else if path.metadata().is_ok_and(|m|m.len() <= 65536) &&
        matches!(path.extension().and_then(|s|s.to_str()).unwrap_or("").to_lowercase().as_str(),
             "md"|"txt"|"rs"|"toml"|"json"|"yaml"|"yml"|"csv"|"py"|"sh"|"html"|"css"|"js"|"ts") {
        if let Ok(data)=fs::read_to_string(path){content.push_str(&data);}
    }
    content.chars().take(16384).collect()
}

/// Explicit refresh, bounded and cycle-safe. Incrementally reuses unchanged entries.
pub fn rebuild(root:&Path)->Result<usize,String> {
    let root=fs::canonicalize(root).map_err(|e|e.to_string())?;
    let prior=read_index(&root).documents.into_iter().map(|d|(d.path.clone(),d)).collect::<HashMap<_,_>>();
    let mut stack=vec![root.clone()];
    let mut visited=HashSet::new();
    let mut docs=Vec::new();
    while let Some(folder)=stack.pop(){
        if !visited.insert(folder.clone()){continue;}
        let Ok(children)=fs::read_dir(&folder) else {continue;};
        for child in children.flatten(){
            let path=child.path();
            let Ok(kind)=child.file_type() else {continue;};
            if kind.is_symlink(){continue;}
            let name=child.file_name();
            if name.to_string_lossy().starts_with('.') {continue;}
            if kind.is_dir() {
                if stack.len()<100000 {stack.push(path);}
            } else if kind.is_file(){
                let Ok(meta)=child.metadata() else {continue;};
                let modified=stamp(meta.modified());
                let len=meta.len();
                if let Some(old)=prior.get(&path).filter(|d|d.modified==modified&&d.len==len){
                    docs.push(old.clone());
                } else { docs.push(Document {content:summarize(&path), path,modified,len}); }
                if docs.len()>=100000 {break;}
            }
        }
        if docs.len()>=100000 {break;}
    }
    let count=docs.len();
    let index=Index{root:root.clone(),documents:docs};
    let target=index_path(&root);
    fs::create_dir_all(target.parent().unwrap_or(Path::new("."))).map_err(|e|e.to_string())?;
    let temp=target.with_extension("tmp");
    fs::write(&temp,serde_json::to_vec(&index).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    fs::rename(temp,target).map_err(|e|e.to_string())?;
    Ok(count)
}

pub fn find(root:&Path,query:&str,limit:usize)->Result<Vec<PathBuf>,String> {
    let root=fs::canonicalize(root).map_err(|e|e.to_string())?;
    let index=read_index(&root);
    if index.documents.is_empty(){return Err("No search index. Run :index in this folder first.".into());}
    let terms=query.to_lowercase().split_whitespace().map(str::to_string).collect::<Vec<_>>();
    if terms.is_empty(){return Ok(Vec::new());}
    let mut matches=index.documents.into_iter().filter_map(|doc|{
        let filename=doc.path.file_name()?.to_string_lossy().to_lowercase();
        let content=doc.content.to_lowercase();
        if !terms.iter().all(|term|filename.contains(term)||content.contains(term)){return None;}
        let relevance=terms.iter().map(|term|if filename.contains(term){10}else{1}).sum::<i32>();
        Some((relevance,doc.path))
    }).collect::<Vec<_>>();
    matches.sort_by(|a,b|b.0.cmp(&a.0).then_with(||a.1.cmp(&b.1)));
    Ok(matches.into_iter().take(limit).map(|(_,path)|path).collect())
}

pub fn clear(root:&Path)->io::Result<()> {
    let root=fs::canonicalize(root)?;
    match fs::remove_file(index_path(&root)){Err(e) if e.kind()==io::ErrorKind::NotFound=>Ok(()),result=>result}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn indexing_searches_names_and_content() {
        let dir=std::env::temp_dir().join(format!("explorer-index-test-{}",std::process::id()));
        let _=fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("notes.md"),"Brutalist architecture study").unwrap();
        fs::write(dir.join("drawing.png"),b"not-a-png").unwrap();
        assert_eq!(rebuild(&dir).unwrap(),2);
        assert_eq!(find(&dir,"brutalist",10).unwrap(),vec![dir.join("notes.md")]);
        clear(&dir).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }
}
