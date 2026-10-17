//! A DirectX `.x` file as the converter needs it: the frames with their matrices, the
//! meshes inside them (positions, faces, normals, texture coordinates, the material list,
//! skin weights) and the materials, named or inline. Text (`xof 0302txt`, `0303txt`) and
//! binary (`...bin`) files alike, with 32- or 64-bit floats; the compressed forms (`bzip`,
//! `tzip`) are refused. Templates are passed over: the data objects are read by their
//! well-known names, anything else is skipped as a block.

use crate::Error;
use std::collections::HashMap;

/// A `Material` data object (`faceColor; power; specularColor; emissiveColor; [TextureFilename]`).
#[derive(Debug, Clone, PartialEq)]
pub struct XMaterial {
    pub diffuse: [f32; 4],
    pub power: f32,
    pub specular: [f32; 3],
    pub emissive: [f32; 3],
    /// The texture's file name as written (empty without one).
    pub texture: String,
}

impl Default for XMaterial {
    fn default() -> Self {
        XMaterial { diffuse: [1.0; 4], power: 0.0, specular: [0.0; 3], emissive: [0.0; 3], texture: String::new() }
    }
}

/// An entry of a mesh's `MeshMaterialList`: a material written there, or `{ Name }` of one
/// defined elsewhere in the file.
#[derive(Debug, Clone, PartialEq)]
pub enum MaterialSlot {
    Inline(XMaterial),
    Reference(String),
}

/// A `SkinWeights` object: the bone's name, the vertices it moves and how much.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SkinWeights {
    pub bone: String,
    pub indices: Vec<u32>,
    pub weights: Vec<f32>,
    pub matrix: [f32; 16],
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct XMesh {
    pub name: String,
    pub positions: Vec<[f32; 3]>,
    /// Each face's vertex indices (three or more).
    pub faces: Vec<Vec<u32>>,
    /// `MeshNormals`: the normals and, per face, which normal each corner uses (empty
    /// without the object).
    pub normals: Vec<[f32; 3]>,
    pub normal_faces: Vec<Vec<u32>>,
    pub uvs: Vec<[f32; 2]>,
    /// `MeshMaterialList`: per face, its material (an index into `materials`).
    pub face_materials: Vec<u32>,
    pub materials: Vec<MaterialSlot>,
    pub skin: Vec<SkinWeights>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Frame {
    pub name: String,
    /// `FrameTransformMatrix`, the 16 numbers as written (rows of a row-vector matrix, the
    /// translation in the last row).
    pub matrix: Option<[f32; 16]>,
    pub meshes: Vec<XMesh>,
    pub children: Vec<Frame>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct XFile {
    /// The file's top level: its frames as children, its meshes outside any frame.
    pub root: Frame,
    /// `Material Name { ... }` objects anywhere in the file, by name.
    pub named_materials: HashMap<String, XMaterial>,
    pub binary: bool,
    pub float64: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Name(String),
    Str(String),
    Int(i64),
    Num(f64),
    Guid,
    Open,
    Close,
    Semi,
    Comma,
    Template,
    Other,
}

/// Read a `.x` file from memory.
pub fn parse(bytes: &[u8]) -> Result<XFile, Error> {
    if bytes.len() < 16 || !bytes.starts_with(b"xof ") {
        return Err(Error::NotX);
    }
    let kind = &bytes[8..12];
    let float64 = &bytes[12..16] == b"0064";
    let toks = match kind {
        b"txt " => lex_text(bytes),
        b"bin " => lex_binary(&bytes[16..], float64)?,
        b"bzip" | b"tzip" => return Err(Error::Compressed(String::from_utf8_lossy(kind).trim().to_string())),
        _ => return Err(Error::NotX),
    };
    let mut p = Parser { t: toks, i: 0 };
    let mut x = XFile { binary: kind == b"bin ", float64, ..Default::default() };
    let mut root = Frame::default();
    p.objects(&mut root, &mut x.named_materials, true)?;
    x.root = root;
    Ok(x)
}

fn lex_text(bytes: &[u8]) -> Vec<Tok> {
    let (text, _) = encoding_rs::WINDOWS_1252.decode_without_bom_handling(&bytes[16..]);
    let s: Vec<char> = text.chars().collect();
    let mut out = Vec::with_capacity(s.len() / 4);
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        match c {
            ' ' | '\t' | '\r' | '\n' => i += 1,
            '/' if s.get(i + 1) == Some(&'/') => {
                while i < s.len() && s[i] != '\n' {
                    i += 1;
                }
            }
            '#' => {
                while i < s.len() && s[i] != '\n' {
                    i += 1;
                }
            }
            '"' => {
                let start = i + 1;
                i += 1;
                while i < s.len() && s[i] != '"' {
                    i += 1;
                }
                out.push(Tok::Str(s[start..i.min(s.len())].iter().collect()));
                i += 1;
            }
            '<' => {
                while i < s.len() && s[i] != '>' {
                    i += 1;
                }
                i += 1;
                out.push(Tok::Guid);
            }
            '{' => {
                out.push(Tok::Open);
                i += 1;
            }
            '}' => {
                out.push(Tok::Close);
                i += 1;
            }
            ';' => {
                out.push(Tok::Semi);
                i += 1;
            }
            ',' => {
                out.push(Tok::Comma);
                i += 1;
            }
            '[' | ']' | '(' | ')' | '.' => {
                out.push(Tok::Other);
                i += 1;
            }
            c if c.is_ascii_digit() || ((c == '-' || c == '+' || c == '.') && s.get(i + 1).is_some_and(|d| d.is_ascii_digit() || *d == '.')) => {
                let start = i;
                i += 1;
                while i < s.len() && (s[i].is_ascii_digit() || matches!(s[i], '.' | 'e' | 'E' | '+' | '-')) {
                    // (a sign only right after an exponent)
                    if matches!(s[i], '+' | '-') && !matches!(s[i - 1], 'e' | 'E') {
                        break;
                    }
                    i += 1;
                }
                let w: String = s[start..i].iter().collect();
                if w.contains(['.', 'e', 'E']) {
                    out.push(Tok::Num(w.parse::<f64>().unwrap_or(0.0)));
                } else {
                    out.push(Tok::Int(w.parse::<i64>().unwrap_or(0)));
                }
            }
            c if c.is_alphanumeric() || c == '_' => {
                let start = i;
                while i < s.len() && (s[i].is_alphanumeric() || matches!(s[i], '_' | '-' | '.')) {
                    i += 1;
                }
                let w: String = s[start..i].iter().collect();
                out.push(if w == "template" { Tok::Template } else { Tok::Name(w) });
            }
            _ => i += 1,
        }
    }
    out
}

fn lex_binary(b: &[u8], float64: bool) -> Result<Vec<Tok>, Error> {
    let mut out = Vec::with_capacity(b.len() / 4);
    let mut i = 0;
    let u16_at = |i: usize| -> Option<u16> { b.get(i..i + 2).map(|s| u16::from_le_bytes([s[0], s[1]])) };
    let u32_at = |i: usize| -> Option<u32> { b.get(i..i + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]])) };
    let text = |s: &[u8]| encoding_rs::WINDOWS_1252.decode_without_bom_handling(s).0.into_owned();
    while let Some(t) = u16_at(i) {
        i += 2;
        match t {
            1 | 2 => {
                let n = u32_at(i).ok_or_else(|| Error::Syntax("cut short".into(), out.len()))? as usize;
                i += 4;
                let s = b.get(i..i + n).ok_or_else(|| Error::Syntax("cut short".into(), out.len()))?;
                i += n;
                if t == 1 {
                    let w = text(s);
                    out.push(if w == "template" { Tok::Template } else { Tok::Name(w) });
                } else {
                    out.push(Tok::Str(text(s)));
                    // (a string carries its own terminator)
                    if let Some(term) = u16_at(i) {
                        i += 2;
                        out.push(if term == 19 { Tok::Comma } else { Tok::Semi });
                    }
                }
            }
            3 => {
                out.push(Tok::Int(u32_at(i).ok_or_else(|| Error::Syntax("cut short".into(), out.len()))? as i64));
                i += 4;
            }
            5 => {
                i += 16;
                out.push(Tok::Guid);
            }
            6 => {
                let n = u32_at(i).ok_or_else(|| Error::Syntax("cut short".into(), out.len()))? as usize;
                i += 4;
                for _ in 0..n {
                    out.push(Tok::Int(u32_at(i).ok_or_else(|| Error::Syntax("cut short".into(), out.len()))? as i64));
                    i += 4;
                }
            }
            7 => {
                let n = u32_at(i).ok_or_else(|| Error::Syntax("cut short".into(), out.len()))? as usize;
                i += 4;
                for _ in 0..n {
                    if float64 {
                        let s = b.get(i..i + 8).ok_or_else(|| Error::Syntax("cut short".into(), out.len()))?;
                        out.push(Tok::Num(f64::from_le_bytes(s.try_into().unwrap())));
                        i += 8;
                    } else {
                        out.push(Tok::Num(f32::from_bits(u32_at(i).ok_or_else(|| Error::Syntax("cut short".into(), out.len()))?) as f64));
                        i += 4;
                    }
                }
            }
            10 => out.push(Tok::Open),
            11 => out.push(Tok::Close),
            19 => out.push(Tok::Comma),
            20 => out.push(Tok::Semi),
            31 => out.push(Tok::Template),
            12..=18 => out.push(Tok::Other),
            40..=52 => out.push(Tok::Name(String::new())),
            _ => return Err(Error::Syntax(format!("unknown binary token {t}"), out.len())),
        }
    }
    Ok(out)
}

struct Parser {
    t: Vec<Tok>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.t.get(self.i).cloned();
        self.i += 1;
        t
    }

    fn fail<T>(&self, what: &str) -> Result<T, Error> {
        Err(Error::Syntax(what.into(), self.i))
    }

    fn seps(&mut self) {
        while matches!(self.peek(), Some(Tok::Semi | Tok::Comma)) {
            self.i += 1;
        }
    }

    fn int(&mut self) -> Result<u32, Error> {
        self.seps();
        match self.next() {
            Some(Tok::Int(v)) => Ok(v.max(0) as u32),
            Some(Tok::Num(v)) => Ok(v.max(0.0) as u32),
            _ => self.fail("a count was expected"),
        }
    }

    fn num(&mut self) -> Result<f32, Error> {
        self.seps();
        match self.next() {
            Some(Tok::Num(v)) => Ok(v as f32),
            Some(Tok::Int(v)) => Ok(v as f32),
            _ => self.fail("a number was expected"),
        }
    }

    fn string(&mut self) -> Result<String, Error> {
        self.seps();
        match self.next() {
            Some(Tok::Str(s)) => Ok(s),
            Some(Tok::Name(s)) => Ok(s),
            _ => self.fail("a string was expected"),
        }
    }

    /// The rest of an object's header after its type: an optional name, an optional GUID,
    /// then the opening brace. The name.
    fn header(&mut self) -> Result<String, Error> {
        let mut name = String::new();
        loop {
            match self.next() {
                Some(Tok::Open) => return Ok(name),
                Some(Tok::Name(n)) => name = n,
                Some(Tok::Guid) | Some(Tok::Other) => {}
                _ => return self.fail("an object's `{` was expected"),
            }
        }
    }

    /// Past the matching `}` of a block whose `{` was consumed.
    fn skip_block(&mut self) -> Result<(), Error> {
        let mut depth = 1;
        while let Some(t) = self.next() {
            match t {
                Tok::Open => depth += 1,
                Tok::Close => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
        self.fail("a block is not closed")
    }

    /// Skip an object whose type token was consumed (`Name [name] [<guid>] { ... }`), or a
    /// bare reference `{ name }`.
    fn skip_object(&mut self) -> Result<(), Error> {
        self.header()?;
        self.skip_block()
    }

    /// The objects of a frame (or the file, `top`) up to its `}`.
    fn objects(&mut self, frame: &mut Frame, named: &mut HashMap<String, XMaterial>, top: bool) -> Result<(), Error> {
        loop {
            let Some(t) = self.next() else {
                return if top { Ok(()) } else { self.fail("a frame is not closed") };
            };
            match t {
                Tok::Close => return Ok(()),
                Tok::Template => {
                    self.header()?;
                    self.skip_block()?;
                }
                Tok::Name(n) => match n.as_str() {
                    "Frame" => {
                        let name = self.header()?;
                        let mut child = Frame { name, ..Default::default() };
                        self.objects(&mut child, named, false)?;
                        frame.children.push(child);
                    }
                    "FrameTransformMatrix" => {
                        self.header()?;
                        let mut m = [0f32; 16];
                        for v in m.iter_mut() {
                            *v = self.num()?;
                        }
                        self.skip_block()?;
                        frame.matrix = Some(m);
                    }
                    "Mesh" => {
                        let name = self.header()?;
                        let mesh = self.mesh(name, named)?;
                        frame.meshes.push(mesh);
                    }
                    "Material" => {
                        let (name, m) = self.material()?;
                        if !name.is_empty() {
                            named.insert(name, m);
                        }
                    }
                    _ => self.skip_object()?,
                },
                Tok::Open => self.skip_block()?,
                _ => {}
            }
        }
    }

    fn mesh(&mut self, name: String, named: &mut HashMap<String, XMaterial>) -> Result<XMesh, Error> {
        let mut m = XMesh { name, ..Default::default() };
        let nv = self.int()? as usize;
        let room = self.t.len().saturating_sub(self.i);
        m.positions.reserve(nv.min(room / 3));
        for _ in 0..nv {
            m.positions.push([self.num()?, self.num()?, self.num()?]);
        }
        let nf = self.int()? as usize;
        m.faces.reserve(nf.min(room / 4));
        for _ in 0..nf {
            let n = self.int()? as usize;
            let mut f = Vec::with_capacity(n.min(64));
            for _ in 0..n {
                f.push(self.int()?);
            }
            m.faces.push(f);
        }
        loop {
            self.seps();
            let Some(t) = self.next() else { return self.fail("a mesh is not closed") };
            match t {
                Tok::Close => break,
                Tok::Name(n) => match n.as_str() {
                    "MeshNormals" => {
                        self.header()?;
                        let nn = self.int()? as usize;
                        for _ in 0..nn {
                            m.normals.push([self.num()?, self.num()?, self.num()?]);
                        }
                        let nfn = self.int()? as usize;
                        for _ in 0..nfn {
                            let k = self.int()? as usize;
                            let mut f = Vec::with_capacity(k.min(64));
                            for _ in 0..k {
                                f.push(self.int()?);
                            }
                            m.normal_faces.push(f);
                        }
                        self.skip_block()?;
                    }
                    "MeshTextureCoords" => {
                        self.header()?;
                        let n = self.int()? as usize;
                        for _ in 0..n {
                            m.uvs.push([self.num()?, self.num()?]);
                        }
                        self.skip_block()?;
                    }
                    "MeshMaterialList" => {
                        self.header()?;
                        let _nm = self.int()?;
                        let nfi = self.int()? as usize;
                        for _ in 0..nfi {
                            m.face_materials.push(self.int()?);
                        }
                        loop {
                            self.seps();
                            match self.next() {
                                Some(Tok::Close) => break,
                                Some(Tok::Name(n)) if n == "Material" => {
                                    let (name, mat) = self.material()?;
                                    if !name.is_empty() {
                                        named.insert(name, mat.clone());
                                    }
                                    m.materials.push(MaterialSlot::Inline(mat));
                                }
                                Some(Tok::Name(_)) => self.skip_object()?,
                                Some(Tok::Open) => {
                                    // `{ Name }`: a material defined elsewhere
                                    let name = match self.next() {
                                        Some(Tok::Name(n)) => n,
                                        _ => return self.fail("a material reference without a name"),
                                    };
                                    m.materials.push(MaterialSlot::Reference(name));
                                    self.skip_block()?;
                                }
                                Some(_) => {}
                                None => return self.fail("a material list is not closed"),
                            }
                        }
                    }
                    "SkinWeights" => {
                        self.header()?;
                        let bone = self.string()?;
                        let n = self.int()? as usize;
                        let mut w = SkinWeights { bone, ..Default::default() };
                        for _ in 0..n {
                            w.indices.push(self.int()?);
                        }
                        for _ in 0..n {
                            w.weights.push(self.num()?);
                        }
                        for v in w.matrix.iter_mut() {
                            *v = self.num()?;
                        }
                        self.skip_block()?;
                        m.skin.push(w);
                    }
                    _ => self.skip_object()?,
                },
                Tok::Open => self.skip_block()?,
                _ => {}
            }
        }
        Ok(m)
    }

    /// A `Material` after its type token: its name (empty when inline) and its values.
    fn material(&mut self) -> Result<(String, XMaterial), Error> {
        let name = self.header()?;
        let mut m = XMaterial { diffuse: [self.num()?, self.num()?, self.num()?, self.num()?], ..Default::default() };
        m.power = self.num()?;
        m.specular = [self.num()?, self.num()?, self.num()?];
        m.emissive = [self.num()?, self.num()?, self.num()?];
        loop {
            self.seps();
            match self.next() {
                Some(Tok::Close) => break,
                Some(Tok::Name(n)) if n == "TextureFilename" => {
                    self.header()?;
                    m.texture = self.string()?;
                    self.skip_block()?;
                }
                Some(Tok::Name(_)) => self.skip_object()?,
                Some(Tok::Open) => self.skip_block()?,
                Some(_) => {}
                None => return self.fail("a material is not closed"),
            }
        }
        Ok((name, m))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOX: &str = "xof 0303txt 0032\n\
template Mesh { <3d82ab44-62da-11cf-ab39-0020af71e433> DWORD nVertices; array Vector vertices[nVertices]; }\n\
Material Red { 1.0;0.0;0.0;1.0;; 8.0; 0.5;0.5;0.5;; 0.1;0.1;0.1;; TextureFilename { \"red.bmp\"; } }\n\
Frame Root { // the root\n\
  FrameTransformMatrix { 1,0,0,0, 0,0,1,0, 0,1,0,0, 0,0,0,1;; }\n\
  Frame Box {\n\
    FrameTransformMatrix relative { 2,0,0,0, 0,2,0,0, 0,0,2,0, 0.5,1.5,2.5,1;; }\n\
    Mesh Cube {\n\
      4;\n 0;0;0;,\n 1;0;0;,\n 1;1;0;,\n 0;1;0;;\n\
      2;\n 3;0,1,2;,\n 3;0,2,3;;\n\
      MeshNormals { 1; 0;0;1;; 2; 3;0,0,0;, 3;0,0,0;; }\n\
      MeshTextureCoords { 4; 0;0;, 1;0;, 1;1;, 0;1;; }\n\
      MeshMaterialList { 2; 2; 0, 1;; Material { 1;1;1;1;; 0; 0;0;0;; 0;0;0;; } { Red } }\n\
      SkinWeights { \"Bone.001\"; 2; 0, 3; 1.0, 0.5; 1,0,0,0, 0,1,0,0, 0,0,1,0, 0,0,0,1;; }\n\
      VertexDuplicationIndices { 4; 4; 0, 1, 2, 3; }\n\
    }\n\
  }\n\
}\n";

    #[test]
    fn a_text_file_is_read_whole() {
        let x = parse(BOX.as_bytes()).unwrap();
        assert!(!x.binary && !x.float64);
        assert_eq!(x.named_materials["Red"].texture, "red.bmp");
        assert_eq!(x.named_materials["Red"].power, 8.0);
        let root = &x.root.children[0];
        assert_eq!(root.name, "Root");
        assert_eq!(root.matrix.unwrap()[6], 1.0);
        let b = &root.children[0];
        assert_eq!(b.matrix.unwrap()[12..15], [0.5, 1.5, 2.5]);
        let m = &b.meshes[0];
        assert_eq!(m.name, "Cube");
        assert_eq!(m.positions.len(), 4);
        assert_eq!(m.faces, vec![vec![0, 1, 2], vec![0, 2, 3]]);
        assert_eq!(m.normals, vec![[0.0, 0.0, 1.0]]);
        assert_eq!(m.normal_faces.len(), 2);
        assert_eq!(m.uvs[2], [1.0, 1.0]);
        assert_eq!(m.face_materials, vec![0, 1]);
        assert_eq!(m.materials.len(), 2);
        assert_eq!(m.materials[1], MaterialSlot::Reference("Red".into()));
        assert_eq!(m.skin[0].bone, "Bone.001");
        assert_eq!(m.skin[0].indices, vec![0, 3]);
        assert_eq!(m.skin[0].weights, vec![1.0, 0.5]);
    }

    /// The same box as a binary file, written token by token.
    fn binary_box() -> Vec<u8> {
        let mut b = b"xof 0302bin 0032".to_vec();
        let tok = |b: &mut Vec<u8>, t: u16| b.extend_from_slice(&t.to_le_bytes());
        let name = |b: &mut Vec<u8>, s: &str| {
            tok(b, 1);
            b.extend_from_slice(&(s.len() as u32).to_le_bytes());
            b.extend_from_slice(s.as_bytes());
        };
        let string = |b: &mut Vec<u8>, s: &str| {
            tok(b, 2);
            b.extend_from_slice(&(s.len() as u32).to_le_bytes());
            b.extend_from_slice(s.as_bytes());
            tok(b, 20);
        };
        let ints = |b: &mut Vec<u8>, v: &[u32]| {
            tok(b, 6);
            b.extend_from_slice(&(v.len() as u32).to_le_bytes());
            for x in v {
                b.extend_from_slice(&x.to_le_bytes());
            }
        };
        let floats = |b: &mut Vec<u8>, v: &[f32]| {
            tok(b, 7);
            b.extend_from_slice(&(v.len() as u32).to_le_bytes());
            for x in v {
                b.extend_from_slice(&x.to_le_bytes());
            }
        };
        name(&mut b, "Frame");
        name(&mut b, "Box");
        tok(&mut b, 10);
        name(&mut b, "FrameTransformMatrix");
        tok(&mut b, 10);
        floats(&mut b, &[2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.5, 1.5, 2.5, 1.0]);
        tok(&mut b, 11);
        name(&mut b, "Mesh");
        tok(&mut b, 10);
        ints(&mut b, &[3]);
        floats(&mut b, &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0]);
        ints(&mut b, &[1, 3, 0, 1, 2]);
        name(&mut b, "MeshMaterialList");
        tok(&mut b, 10);
        ints(&mut b, &[1, 1, 0]);
        name(&mut b, "Material");
        tok(&mut b, 10);
        floats(&mut b, &[0.2, 0.4, 0.6, 1.0, 5.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        name(&mut b, "TextureFilename");
        tok(&mut b, 10);
        string(&mut b, "box.png");
        tok(&mut b, 11);
        tok(&mut b, 11);
        tok(&mut b, 11);
        tok(&mut b, 11);
        tok(&mut b, 11);
        b
    }

    #[test]
    fn a_binary_file_reads_the_same_way() {
        let x = parse(&binary_box()).unwrap();
        assert!(x.binary);
        let f = &x.root.children[0];
        assert_eq!(f.name, "Box");
        assert_eq!(f.matrix.unwrap()[0], 2.0);
        let m = &f.meshes[0];
        assert_eq!(m.positions, vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]]);
        assert_eq!(m.faces, vec![vec![0, 1, 2]]);
        match &m.materials[0] {
            MaterialSlot::Inline(mat) => {
                assert_eq!(mat.texture, "box.png");
                assert!((mat.diffuse[1] - 0.4).abs() < 1e-6 && mat.power == 5.0);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn what_is_not_an_x_file_is_refused() {
        assert!(matches!(parse(b"hello").unwrap_err(), Error::NotX));
        assert!(matches!(parse(b"xof 0303bzip0032....").unwrap_err(), Error::Compressed(_)));
        assert!(parse(b"xof 0303txt 0032\nMesh { 3; 0;0;0;, 1;").is_err());
    }
}
