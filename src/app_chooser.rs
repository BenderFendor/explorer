//! Lightweight desktop-entry chooser based on the XDG MIME Apps convention.
//! Desktop files are launched by GIO, not by interpreting Exec lines ourselves.
use std::{collections::HashSet,fs,path::{Path,PathBuf},process::Command};

#[derive(Clone,Debug,PartialEq)]
pub struct App {
    pub id:String,
    pub name:String,
    pub desktop:PathBuf,
}

pub fn list(path:&Path)->Vec<App>{
    let mime=mime_guess::from_path(path).first_or_octet_stream().to_string();
    let xdg_data=std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_|"/usr/local/share:/usr/share".into());
    let mut dirs=vec![directories::BaseDirs::new()
        .map(|d|d.data_local_dir().to_path_buf()).unwrap_or_else(||PathBuf::from("/nonexistent"))];
    dirs.extend(xdg_data.split(':').map(PathBuf::from));
    let mut seen=HashSet::new();
    let mut apps=Vec::new();
    for dir in dirs {
        let root=dir.join("applications");
        let mut queue=vec![root.clone()];
        while let Some(folder)=queue.pop(){
            let Ok(entries)=fs::read_dir(&folder)else{continue;};
            for e in entries.flatten(){
                let file=e.path();
                if file.is_dir(){queue.push(file);continue;}
                if file.extension().is_none_or(|x|x!="desktop"){continue;}
                let Some(id)=file.strip_prefix(&root).ok().map(|p|p.to_string_lossy().replace('/',"-")) else {continue;};
                if seen.contains(&id){continue;}
                let Ok(text)=fs::read_to_string(&file) else{continue;};
                let mut section=false;let mut name=None;let mut supported=false;let mut hidden=false;
                for line in text.lines().map(str::trim) {
                    if line.starts_with('['){section=line=="[Desktop Entry]";continue;}
                    if !section{continue;}
                    if let Some(v)=line.strip_prefix("Name="){name=Some(v.to_owned());}
                    if let Some(v)=line.strip_prefix("MimeType="){supported=v.split(';').any(|m|m==mime);}
                    if matches!(line,"Hidden=true"|"NoDisplay=true"){hidden=true;}
                }
                seen.insert(id.clone());
                if supported && !hidden {
                    let title=name.unwrap_or_else(||id.clone());
                    apps.push(App{id,name:title,desktop:file});
                }
            }
        }
    }
    apps.sort_by(|a,b|a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
}
pub fn launch(app:&App, files:&[PathBuf])->Result<(),String>{
    // gio launch consumes a desktop file path and file arguments, no shell involved.
    Command::new("gio").arg("launch").arg(&app.desktop).args(files)
        .spawn().map(|_|()).map_err(|e|e.to_string())
}
pub fn set_default(app:&App,path:&Path)->Result<(),String>{
    let mime=mime_guess::from_path(path).first_or_octet_stream().to_string();
    let status=Command::new("xdg-mime").arg("default").arg(&app.id).arg(mime)
        .status().map_err(|e|e.to_string())?;
    if status.success(){Ok(())}else{Err("xdg-mime rejected the default association".into())}
}
