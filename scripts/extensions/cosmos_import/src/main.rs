//! ZIP importer for Cosmos collections, which currently export images only.
use serde::{Serialize,Deserialize};
use std::{fs,io::{self,Read,Write},path::{Path,PathBuf}};
#[derive(Deserialize)]
struct Request {api_version:u32,action:String,directory:PathBuf,arguments:String}
#[derive(Serialize)]
struct Response{message:String,refresh:bool,updates:Vec<serde_json::Value>}
fn unique(root:&Path,name:&str)->io::Result<(PathBuf,fs::File)>{
    let base=Path::new(name);
    let stem=base.file_stem().unwrap_or_default().to_string_lossy();
    let ext=base.extension().unwrap_or_default().to_string_lossy();
    for index in 0..10000{
        let name=if index==0{format!("{stem}.{ext}")}else{format!("{stem} ({index}).{ext}")};
        let target=root.join(name);
        match fs::OpenOptions::new().write(true).create_new(true).open(&target){
            Ok(file)=>return Ok((target,file)),
            Err(e) if e.kind()==io::ErrorKind::AlreadyExists=>continue,
            Err(e)=>return Err(e),
        }
    }
    Err(io::Error::new(io::ErrorKind::AlreadyExists,"Too many duplicate files"))
}
fn run()->Result<(),String>{
    let mut input=String::new();io::stdin().read_to_string(&mut input).map_err(|e|e.to_string())?;
    let req:Request=serde_json::from_str(&input).map_err(|e|e.to_string())?;
    if req.api_version!=1||req.action!="run"{return Err("Unsupported Explorer API".into());}
    let arg=PathBuf::from(req.arguments.trim());
    if req.arguments.trim().is_empty(){return Err("Usage: :script cosmos_import EXPORT.zip".into());}
    let source=if arg.is_absolute(){arg}else{req.directory.join(arg)};
    let file=fs::File::open(source).map_err(|e|e.to_string())?;
    let mut archive=zip::ZipArchive::new(file).map_err(|e|e.to_string())?;
    let mut imported=0;let mut skipped=0;let mut total=0u64;
    for index in 0..archive.len().min(10000){
        let mut item=archive.by_index(index).map_err(|e|e.to_string())?;
        if item.is_dir(){continue;}
        let Some(path)=item.enclosed_name() else{skipped+=1;continue;};
        let Some(filename)=path.file_name().and_then(|s|s.to_str()) else{skipped+=1;continue;};
        let ext=Path::new(filename).extension().and_then(|s|s.to_str()).unwrap_or("").to_lowercase();
        if !matches!(ext.as_str(),"jpg"|"jpeg"|"png"|"webp"|"gif"|"avif"|"bmp"){skipped+=1;continue;}
        if item.size()>50_000_000 || total+item.size()>2_000_000_000{skipped+=1;continue;}
        let (target,mut dest)=unique(&req.directory,filename).map_err(|e|e.to_string())?;
        let copied=io::copy(&mut item,&mut dest).map_err(|e|e.to_string());
        match copied {
            Ok(size)=>{total+=size;imported+=1;}
            Err(e)=>{let _=fs::remove_file(target);return Err(e);}
        }
    }
    println!("{}",serde_json::to_string(&Response{
        message:format!("Imported {imported} Cosmos images ({skipped} skipped). Cosmos ZIPs do not include source URLs or captions."),
        refresh:true,updates:vec![]
    }).map_err(|e|e.to_string())?);
    Ok(())
}
fn main(){if let Err(e)=run(){eprintln!("{e}");std::process::exit(1)}}
