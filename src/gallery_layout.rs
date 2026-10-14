//! Geometry for a virtualized masonry grid and an optional folder-local board.
//! Only small image headers are read; no decoding or background indexing is required.
use std::{fs::File, io::Read, path::{Path, PathBuf}};
use crate::image_metadata::{self, Board, Card};

#[derive(Clone, Debug, PartialEq)]
pub struct Rect { pub x:f64, pub y:f64, pub w:f64, pub h:f64 }
#[derive(Clone, Debug, PartialEq, Default)]
pub struct GalleryLayout { pub rects: Vec<Rect>, pub height: f64 }

impl GalleryLayout {
    pub fn build(paths:&[PathBuf], width:f64, zoom:f64, board:bool, folder:&Path) -> Self {
        let gap=12.0;
        let w=(210.0*zoom).clamp(125.0, 520.0);
        let columns=((width+gap)/(w+gap)).floor().max(1.0) as usize;
        let col_w=(width-(columns-1) as f64*gap)/columns as f64;
        let mut bottom=vec![0.0_f64;columns];
        let existing=if board { image_metadata::load_board(folder) } else { Board::default() };
        let mut rects=Vec::with_capacity(paths.len());
        for path in paths {
            let (iw,ih)=dimensions(path).unwrap_or((1,1));
            let ratio=(ih as f64 / iw.max(1) as f64).clamp(0.36,3.0);
            let card_h=col_w*ratio + 25.0;
            let (col,y)=bottom.iter().enumerate().min_by(|a,b|a.1.total_cmp(b.1)).map(|(col,y)|(col,*y)).unwrap_or((0,0.0));
            let mut rect=Rect{x:col as f64*(col_w+gap),y,w:col_w,h:card_h};
            bottom[col]=y+card_h+gap;
            if board {
                let name=path.file_name().map(|s|s.to_string_lossy().to_string()).unwrap_or_default();
                if let Some(card)=existing.cards.iter().find(|card|card.file==name) {
                    rect.x=(card.x as f64).max(0.0);
                    rect.y=(card.y as f64).max(0.0);
                    if card.width>0.0 {rect.w=(card.width as f64).clamp(80.0,2000.0);rect.h=rect.w*ratio+25.0;}
                }
            }
            rects.push(rect);
        }
        let height=rects.iter().map(|r|r.y+r.h).fold(0.0_f64,f64::max);
        Self{rects,height}
    }
    pub fn visible(&self,scroll:f64,height:f64)->Vec<usize> {
        self.rects.iter().enumerate().filter(|(_,r)|r.y+r.h>=scroll-250.0 && r.y<=scroll+height+250.0).map(|(i,_)|i).collect()
    }
    pub fn hit(&self,x:f64,y:f64)->Option<usize>{
        // Last drawn card is topmost on overlapping boards.
        self.rects.iter().enumerate().rev().find(|(_,r)|x>=r.x&&x<=r.x+r.w&&y>=r.y&&y<=r.y+r.h).map(|(i,_)|i)
    }
    pub fn max_scroll(&self,viewport:f64)->f64 {(self.height-viewport).max(0.0)}
    pub fn follow(&self,index:usize,scroll:f64,viewport:f64)->f64 {
        let Some(rect)=self.rects.get(index) else {return scroll;};
        let target=if rect.y<scroll+20.0{rect.y-20.0}
          else if rect.y+rect.h>scroll+viewport-20.0{rect.y+rect.h-viewport+20.0}
          else{scroll};
        target.clamp(0.0,self.max_scroll(viewport))
    }
}

/// Changes a card's position without moving the underlying image file.
pub fn move_card(folder:&Path,file:&Path,x:f64,y:f64,width:f64)->std::io::Result<()>{
    let mut board=image_metadata::load_board(folder);
    board.version=1;
    let name=file.file_name().ok_or_else(||std::io::Error::other("Missing filename"))?.to_string_lossy().to_string();
    if !board.cards.iter().any(|c|c.file==name){board.cards.push(Card{file:name.clone(),..Card::default()});}
    let card=board.cards.iter_mut().find(|c|c.file==name).unwrap();
    card.x=x.max(0.0).min(100000.0) as f32;card.y=y.max(0.0).min(100000.0) as f32;
    card.width=width.clamp(80.0,2000.0) as f32;
    image_metadata::save_board(folder,&board)
}

/// Decode only format headers, not entire images. Unknown formats use a square fallback.
pub fn dimensions(path:&Path)->Option<(u32,u32)> {
    let mut f=File::open(path).ok()?;
    let mut bytes=vec![0u8;256*1024];
    let n=f.read(&mut bytes).ok()?;bytes.truncate(n);
    let b=&bytes;
    if b.len()>=24 && b.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some((u32::from_be_bytes(b[16..20].try_into().ok()?),u32::from_be_bytes(b[20..24].try_into().ok()?)));
    }
    if b.len()>=10 && (b.starts_with(b"GIF87a")||b.starts_with(b"GIF89a")) {
        return Some((u16::from_le_bytes(b[6..8].try_into().ok()?) as u32,u16::from_le_bytes(b[8..10].try_into().ok()?) as u32));
    }
    if b.len()>=30 && b.starts_with(b"RIFF") && &b[8..12]==b"WEBP" && &b[12..16]==b"VP8X" {
        let w=1+(b[24] as u32|(b[25] as u32)<<8|(b[26] as u32)<<16);
        let h=1+(b[27] as u32|(b[28] as u32)<<8|(b[29] as u32)<<16);
        return Some((w,h));
    }
    if b.starts_with(&[0xff,0xd8]) {
        let mut p=2;
        while p+9 < b.len() {
            if b[p]!=0xff {p+=1;continue;}
            let marker=b[p+1];p+=2;
            if marker==0xd9||marker==0xda {break;}
            if p+2>b.len(){break;}
            let len=u16::from_be_bytes([b[p],b[p+1]]) as usize;
            if len<2||p+len>b.len(){break;}
            if matches!(marker,0xc0|0xc1|0xc2|0xc3|0xc5|0xc6|0xc7|0xc9|0xca|0xcb|0xcd|0xce|0xcf) && len>=7 {
                let h=u16::from_be_bytes([b[p+3],b[p+4]]) as u32;
                let w=u16::from_be_bytes([b[p+5],b[p+6]]) as u32;
                if w>0&&h>0{return Some((w,h));}
            }
            p+=len;
        }
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn layout_positions_do_not_overlap_in_masonry() {
        let a=vec![PathBuf::from("/does/not/exist.jpg");30];
        let l=GalleryLayout::build(&a,900.0,1.0,false,Path::new("/tmp"));
        assert_eq!(l.rects.len(),30);
        for (i,r) in l.rects.iter().enumerate() {
            for o in l.rects.iter().skip(i+1){
                assert!(r.x+r.w<=o.x || o.x+o.w<=r.x || r.y+r.h<=o.y || o.y+o.h<=r.y);
            }
        }
        assert!(l.visible(0.0,300.0).len()<30);
    }
}
