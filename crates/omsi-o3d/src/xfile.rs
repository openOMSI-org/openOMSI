//! DirectX `.x` mesh reader for the OMSI helper meshes and the scenery objects that still ship
//! `.x` files: frames, Mesh, MeshNormals, MeshTextureCoords, MeshMaterialList, Material,
//! TextureFilename and DeclData (normals and texture coordinates given as a Direct3D 9 vertex
//! declaration). Text files and uncompressed binary files feed the same token stream; the
//! compressed variants (`tzip`, `bzip`) are not read.

use super::{Material, Mesh, O3dError, Triangle, Vertex};
use glam::{Mat3, Mat4, Vec2, Vec3, Vec4};
use std::borrow::Cow;
use std::collections::HashMap;

const MAX_FRAME_DEPTH: usize = 128;

/// Single-character marks of the text format.
const MARKS: &[u8] = b"{};,<>";

/// One token, whichever encoding it came from.
#[derive(Clone, Debug, PartialEq)]
enum Token<'a> {
    Mark(u8),
    /// a name or keyword; in text files numbers are words too, read by `Tok::scalar`
    Word(Cow<'a, str>),
    Quoted(Cow<'a, str>),
    Int(u32),
    Float(f64),
    /// a binary GUID (the text form is `<`, the GUID as a word, `>`)
    Guid,
}

impl Token<'_> {
    /// What the parser matches against: the mark or the word; numbers and strings match none.
    fn key(&self) -> &str {
        match self {
            Token::Mark(m) => match m {
                b'{' => "{",
                b'}' => "}",
                b';' => ";",
                b',' => ",",
                b'<' => "<",
                b'>' => ">",
                b'(' => "(",
                b')' => ")",
                b'[' => "[",
                b']' => "]",
                _ => ".",
            },
            Token::Word(w) => w,
            _ => "",
        }
    }
}

#[derive(Clone, Copy)]
enum Source<'a> {
    Text(&'a str),
    /// little-endian binary tokens; `doubles` when the header gives 64-bit floats
    Binary { data: &'a [u8], doubles: bool },
}

#[derive(Clone, Copy, Default)]
struct Cursor {
    at: usize,
    /// numbers still to come from the binary integer or float list being read
    list_left: u32,
    list_floats: bool,
    /// the separator stored after a binary string, handed out as the next token
    after_string: Option<u8>,
}

struct Tok<'a> {
    src: Source<'a>,
    cur: Cursor,
    /// why a binary file could not be read on (the parser then sees its end)
    broken: Option<String>,
}

fn is_space(b: u8) -> bool {
    b.is_ascii() && (b as char).is_whitespace()
}

/// Names and strings of OMSI content are Windows-1252.
fn ansi(bytes: &[u8]) -> Cow<'_, str> {
    encoding_rs::WINDOWS_1252.decode_without_bom_handling(bytes).0
}

/// The keywords binary templates use for member types (token values 40 to 52).
const TYPE_WORDS: [&str; 13] = [
    "WORD", "DWORD", "FLOAT", "DOUBLE", "CHAR", "UCHAR", "SWORD", "SDWORD", "void", "string", "unicode", "cstring",
    "array",
];

impl<'a> Tok<'a> {
    fn left(&self) -> usize {
        let len = match self.src {
            Source::Text(s) => s.len(),
            Source::Binary { data, .. } => data.len(),
        };
        len.saturating_sub(self.cur.at)
    }

    fn next(&mut self) -> Option<Token<'a>> {
        match self.src {
            Source::Text(s) => self.next_text(s),
            Source::Binary { data, doubles } => self.next_binary(data, doubles),
        }
    }

    fn next_text(&mut self, s: &'a str) -> Option<Token<'a>> {
        let b = s.as_bytes();
        let mut p = self.cur.at;
        loop {
            while p < b.len() && is_space(b[p]) {
                p += 1;
            }
            let comment = b.get(p) == Some(&b'#') || b[p.min(b.len())..].starts_with(b"//");
            if !comment {
                break;
            }
            while p < b.len() && b[p] != b'\n' {
                p += 1;
            }
        }
        let first = *b.get(p)?;
        if first == b'"' {
            let end = b[p + 1..].iter().position(|&c| c == b'"').map_or(b.len(), |i| p + 1 + i);
            self.cur.at = (end + 1).min(b.len());
            return Some(Token::Quoted(Cow::Borrowed(&s[p + 1..end])));
        }
        if MARKS.contains(&first) {
            self.cur.at = p + 1;
            return Some(Token::Mark(first));
        }
        let end = b[p..]
            .iter()
            .position(|&c| is_space(c) || c == b'"' || MARKS.contains(&c))
            .map_or(b.len(), |i| p + i);
        self.cur.at = end;
        Some(Token::Word(Cow::Borrowed(&s[p..end])))
    }

    /// Token values as the DirectX file format documents them: 1 name, 2 string, 3 integer,
    /// 5 GUID, 6 integer list, 7 float list, 10 to 20 the marks `{ } ( ) [ ] < > . , ;`,
    /// 31 `template`, 40 to 52 the member type keywords.
    fn next_binary(&mut self, data: &'a [u8], doubles: bool) -> Option<Token<'a>> {
        if let Some(mark) = self.cur.after_string.take() {
            return Some(Token::Mark(mark));
        }
        loop {
            if self.cur.list_left > 0 {
                self.cur.list_left -= 1;
                return Some(match (self.cur.list_floats, doubles) {
                    (false, _) => Token::Int(u32::from_le_bytes(self.take(data)?)),
                    (true, false) => Token::Float(f32::from_le_bytes(self.take(data)?) as f64),
                    (true, true) => Token::Float(f64::from_le_bytes(self.take(data)?)),
                });
            }
            if self.cur.at >= data.len() {
                return None;
            }
            let id = u16::from_le_bytes(self.take(data)?);
            return Some(match id {
                1 => Token::Word(ansi(self.counted(data)?)),
                2 => {
                    let text = ansi(self.counted(data)?);
                    let end = u32::from_le_bytes(self.take(data)?);
                    self.cur.after_string = Some(if end == 19 { b',' } else { b';' });
                    Token::Quoted(text)
                }
                3 => Token::Int(u32::from_le_bytes(self.take(data)?)),
                5 => {
                    self.take::<16>(data)?;
                    Token::Guid
                }
                6 | 7 => {
                    let n = u32::from_le_bytes(self.take(data)?);
                    let width = if id == 7 && doubles { 8 } else { 4 };
                    if n as usize > self.left() / width {
                        return self.fail(format!("binary .x list of {n} numbers is longer than the file"));
                    }
                    self.cur.list_left = n;
                    self.cur.list_floats = id == 7;
                    continue;
                }
                10..=20 => Token::Mark(b"{}()[]<>.,;"[id as usize - 10]),
                31 => Token::Word(Cow::Borrowed("template")),
                40..=52 => Token::Word(Cow::Borrowed(TYPE_WORDS[id as usize - 40])),
                _ => return self.fail(format!("unknown binary .x token {id}")),
            });
        }
    }

    fn take<const N: usize>(&mut self, data: &[u8]) -> Option<[u8; N]> {
        match data.get(self.cur.at..self.cur.at.saturating_add(N)) {
            Some(bytes) => {
                self.cur.at += N;
                bytes.try_into().ok()
            }
            None => self.fail("binary .x ends inside a token".into()),
        }
    }

    /// A DWORD length and that many bytes (binary names and strings).
    fn counted(&mut self, data: &'a [u8]) -> Option<&'a [u8]> {
        let n = u32::from_le_bytes(self.take(data)?) as usize;
        match data.get(self.cur.at..self.cur.at.saturating_add(n)) {
            Some(bytes) => {
                self.cur.at += n;
                Some(bytes)
            }
            None => self.fail("binary .x ends inside a name".into()),
        }
    }

    fn fail<T>(&mut self, why: String) -> Option<T> {
        self.broken.get_or_insert(why);
        self.cur = Cursor { at: usize::MAX, ..Cursor::default() };
        None
    }

    /// Whether the next token is `key`, without reading it.
    fn at(&mut self, key: &str) -> bool {
        let save = self.cur;
        let hit = self.next().is_some_and(|t| t.key() == key);
        self.cur = save;
        hit
    }

    fn expect(&mut self, key: &str) -> Result<(), O3dError> {
        match self.next() {
            Some(t) if t.key() == key => Ok(()),
            other => Err(O3dError::XFile(format!("expected '{key}', got {other:?}"))),
        }
    }

    /// The name and the GUID a data object may have before its `{`; gives the name.
    fn label(&mut self) -> Option<String> {
        let mut name = None;
        for _ in 0..2 {
            let save = self.cur;
            match self.next() {
                Some(Token::Guid) => {}
                Some(Token::Word(w)) if name.is_none() => name = Some(w.into_owned()),
                _ => {
                    self.cur = save;
                    break;
                }
            }
        }
        name
    }

    /// Step into a data object: past its label and its `{`.
    fn enter(&mut self) -> Result<(), O3dError> {
        self.label();
        self.expect("{")
    }

    /// The next number, past separators.
    fn scalar(&mut self) -> Result<f64, O3dError> {
        loop {
            match self.next() {
                None => return Err(O3dError::XFile("eof in number".into())),
                Some(Token::Mark(b',' | b';') | Token::Guid) => {}
                Some(Token::Int(v)) => return Ok(v as f64),
                Some(Token::Float(v)) => return Ok(v),
                Some(Token::Word(w)) => {
                    let parsed = w.parse::<f64>();
                    return parsed.map_err(|_| O3dError::XFile(format!("{w:?} is not a number")));
                }
                Some(other) => return Err(O3dError::XFile(format!("expected a number, got {other:?}"))),
            }
        }
    }

    fn number(&mut self) -> Result<f32, O3dError> {
        Ok(self.scalar()? as f32)
    }

    fn int(&mut self) -> Result<usize, O3dError> {
        Ok(self.scalar()? as usize)
    }

    /// A DWORD as stored, all 32 bits of it (DeclData keeps floats as their bit patterns,
    /// which an f32 round trip would change).
    fn bits32(&mut self) -> Result<u32, O3dError> {
        loop {
            match self.next() {
                Some(Token::Mark(b',' | b';')) => {}
                Some(Token::Int(v)) => return Ok(v),
                Some(Token::Word(w)) => {
                    if let Ok(v) = w.parse::<u32>() {
                        return Ok(v);
                    }
                    // some exporters write the signed value
                    return w.parse::<i32>().map(|v| v as u32).map_err(|_| O3dError::XFile(format!("bad DWORD {w}")));
                }
                Some(Token::Float(v)) => return Ok(v as u32),
                other => return Err(O3dError::XFile(format!("expected a DWORD, got {other:?}"))),
            }
        }
    }

    fn bits32_n<const N: usize>(&mut self) -> Result<[u32; N], O3dError> {
        let mut out = [0; N];
        for slot in &mut out {
            *slot = self.bits32()?;
        }
        Ok(out)
    }

    /// Skip to the `}` that closes the object we are in.
    fn skip_block(&mut self) -> Result<(), O3dError> {
        let mut depth = 1;
        while depth > 0 {
            match self.next() {
                Some(Token::Mark(b'{')) => depth += 1,
                Some(Token::Mark(b'}')) => depth -= 1,
                Some(_) => {}
                None => return Err(O3dError::XFile("eof in block".into())),
            }
        }
        Ok(())
    }
}

#[derive(Default)]
struct Ctx {
    mesh: Mesh,
    /// Named data objects in an .x file can be referenced from any frame or mesh.
    named_materials: HashMap<String, Material>,
    /// Resolve after the whole file, because definitions may follow their references.
    material_refs: Vec<(usize, String)>,
}

pub fn parse_x(bytes: &[u8]) -> Result<Mesh, O3dError> {
    // (the 16-byte header is checked as bytes: a short file or a non-ASCII byte there made
    // the text slices below panic)
    if bytes.len() < 16 || !bytes[..16].is_ascii() || !bytes.starts_with(b"xof ") {
        return Err(O3dError::XFile("missing xof header".into()));
    }
    let format: [u8; 4] = bytes[8..12].try_into().unwrap_or_default();
    let text: Cow<str>;
    let src = if format == *b"txt " {
        text = ansi(bytes);
        Source::Text(&text)
    } else if format == *b"bin " {
        Source::Binary { data: bytes, doubles: bytes[12..16] == *b"0064" }
    } else if format == *b"tzip" || format == *b"bzip" {
        return Err(O3dError::XFile("compressed .x not supported".into()));
    } else {
        return Err(O3dError::XFile(format!(".x of the kind {:?} not supported", String::from_utf8_lossy(&format))));
    };
    let mut t = Tok { src, cur: Cursor { at: 16, ..Cursor::default() }, broken: None };
    let mut ctx = Ctx {
        mesh: Mesh {
            transform: Mat4::IDENTITY,
            ..Default::default()
        },
        ..Default::default()
    };
    let parsed = parse_objects(&mut t, &mut ctx, Mat4::IDENTITY);
    if let Some(why) = t.broken {
        return Err(O3dError::XFile(why));
    }
    parsed?;
    for (slot, name) in &ctx.material_refs {
        if let Some(material) = ctx.named_materials.get(name) {
            ctx.mesh.materials[*slot] = material.clone();
        }
    }
    if ctx.mesh.materials.is_empty() {
        ctx.mesh.materials.push(Material::default());
    }
    ctx.mesh.drop_bad_triangles();
    Ok(ctx.mesh)
}

fn parse_objects(t: &mut Tok, ctx: &mut Ctx, xform: Mat4) -> Result<(), O3dError> {
    while let Some(tok) = t.next() {
        match tok.key() {
            "}" => return Ok(()),
            "template" => {
                // template Name { ... }
                t.next();
                t.expect("{")?;
                t.skip_block()?;
            }
            "Frame" => {
                t.enter()?;
                parse_frame(t, ctx, xform, 1)?;
            }
            "Mesh" => {
                t.enter()?;
                parse_mesh(t, ctx, xform)?;
            }
            "Material" => {
                let (name, material) = parse_material(t)?;
                if let Some(name) = name {
                    ctx.named_materials.insert(name, material);
                }
            }
            _ => skip_object(t, &tok)?,
        }
    }
    Ok(())
}

/// An object the reader does not use (`Name [id] { ... }`), after its first token; separators
/// left between objects (the `;;` after a face list) are passed over.
fn skip_object(t: &mut Tok, first: &Token) -> Result<(), O3dError> {
    match first {
        Token::Mark(b'{') => return t.skip_block(),
        Token::Mark(_) => return Ok(()),
        _ => {}
    }
    t.label();
    if t.at("{") {
        t.next();
        t.skip_block()?;
    }
    Ok(())
}

fn parse_frame(t: &mut Tok, ctx: &mut Ctx, parent: Mat4, depth: usize) -> Result<(), O3dError> {
    if depth > MAX_FRAME_DEPTH {
        return Err(O3dError::XFile(format!("frames nested more than {MAX_FRAME_DEPTH} deep")));
    }
    let mut xform = parent;
    loop {
        let tok = t.next().ok_or_else(|| O3dError::XFile("eof in frame".into()))?;
        match tok.key() {
            "}" => return Ok(()),
            "FrameTransformMatrix" => {
                // a data object may carry a name (`FrameTransformMatrix relative {`, the 3ds
                // Max exporter of the Solaris Urbino 18 and the Novi Sad objects)
                t.enter()?;
                let mut m = [0f32; 16];
                for v in m.iter_mut() {
                    *v = t.number()?;
                }
                // the trailing ';;' and the '}'
                t.skip_block()?;
                // Direct3D writes the matrix row by row for row vectors (v' = v·M, the
                // translation in elements 12-14); read column by column that is already the
                // column-vector matrix glam uses. Transposing it as well dropped every
                // translation into the bottom row and turned the rotations the wrong way: the
                // BVG Citaro's Atron terminal and ALMEX (46 meshes 5.5 m forward and 1.3 m up)
                // sat at the bus origin under the floor, and the street name signs hung upside
                // down half way down their poles (their texts were turned by 180° to make up
                // for it; scene.rs no longer does).
                let local = Mat4::from_cols_array(&m);
                xform = parent * local;
            }
            "Frame" => {
                t.enter()?;
                parse_frame(t, ctx, xform, depth + 1)?;
            }
            "Mesh" => {
                t.enter()?;
                parse_mesh(t, ctx, xform)?;
            }
            "Material" => {
                let (name, material) = parse_material(t)?;
                if let Some(name) = name {
                    ctx.named_materials.insert(name, material);
                }
            }
            _ => skip_object(t, &tok)?,
        }
    }
}

/// Normals go by the inverse transpose of the frame matrix.
fn normal_matrix(xform: Mat4) -> Mat3 {
    let linear = Mat3::from_mat4(xform);
    if linear.determinant().abs() > 1e-12 {
        linear.inverse().transpose()
    } else {
        linear
    }
}

fn parse_mesh(t: &mut Tok, ctx: &mut Ctx, xform: Mat4) -> Result<(), O3dError> {
    let base_v = ctx.mesh.vertices.len() as u32;
    let base_m = ctx.mesh.materials.len() as u16;
    let nv = t.int()?;
    // (counts from the file only reserve what the file can hold)
    let mut positions = Vec::with_capacity(nv.min(t.left()));
    for _ in 0..nv {
        let p = Vec3::new(t.number()?, t.number()?, t.number()?);
        positions.push(xform.transform_point3(p));
    }
    let nf = t.int()?;
    let mut faces: Vec<Vec<u32>> = Vec::with_capacity(nf.min(t.left()));
    for _ in 0..nf {
        let n = t.int()?;
        let mut f = Vec::with_capacity(n.min(64));
        for _ in 0..n {
            f.push(t.int()? as u32);
        }
        faces.push(f);
    }
    let mut normals: Vec<Vec3> = vec![Vec3::Z; nv];
    let mut uvs: Vec<Vec2> = vec![Vec2::ZERO; nv];
    let mut face_mats: Vec<u16> = vec![0; nf];
    let mut mats: Vec<Material> = Vec::new();
    let mut material_refs: Vec<(usize, String)> = Vec::new();
    loop {
        let tok = t.next().ok_or_else(|| O3dError::XFile("eof in mesh".into()))?;
        match tok.key() {
            "}" => break,
            "MeshNormals" => {
                t.enter()?;
                let nn = t.int()?;
                let mut nrm = Vec::with_capacity(nn.min(t.left()));
                let to_world = normal_matrix(xform);
                for _ in 0..nn {
                    let n = Vec3::new(t.number()?, t.number()?, t.number()?);
                    nrm.push((to_world * n).normalize_or_zero());
                }
                let nfn = t.int()?;
                for fi in 0..nfn {
                    let n = t.int()?;
                    for k in 0..n {
                        let ni = t.int()?;
                        if fi < faces.len() && k < faces[fi].len() {
                            let vi = faces[fi][k] as usize;
                            if vi < normals.len() && ni < nrm.len() {
                                normals[vi] = nrm[ni];
                            }
                        }
                    }
                }
                t.skip_block()?;
            }
            "MeshTextureCoords" => {
                t.enter()?;
                let n = t.int()?;
                for i in 0..n {
                    let uv = Vec2::new(t.number()?, t.number()?);
                    if i < uvs.len() {
                        uvs[i] = uv;
                    }
                }
                t.skip_block()?;
            }
            "DeclData" => {
                t.enter()?;
                let decl = read_vertex_declaration(t)?;
                t.skip_block()?;
                decl.apply(normal_matrix(xform), &mut normals, &mut uvs);
            }
            "MeshMaterialList" => {
                t.enter()?;
                let _nm = t.int()?;
                let nfi = t.int()?;
                for i in 0..nfi {
                    let m = t.int()? as u16;
                    if i < face_mats.len() {
                        face_mats[i] = m;
                    }
                }
                loop {
                    let tk = t.next().ok_or_else(|| O3dError::XFile("eof in matlist".into()))?;
                    match tk.key() {
                        "}" => break,
                        "Material" => {
                            let (name, material) = parse_material(t)?;
                            if let Some(name) = name {
                                ctx.named_materials.insert(name, material.clone());
                            }
                            mats.push(material);
                        }
                        "{" => {
                            // Keep this slot even if the named material is defined later or
                            // absent: dropping it changes every following face material index.
                            // A reference by GUID alone (`{ <guid> }`) keeps the slot too; it
                            // names nothing this reader can look up.
                            let (mut name, mut by_guid) = (None, false);
                            loop {
                                match t.next() {
                                    Some(Token::Mark(b'}')) => break,
                                    Some(Token::Word(w)) if name.is_none() => name = Some(w.into_owned()),
                                    Some(Token::Mark(b'{')) | None => {
                                        return Err(O3dError::XFile("unclosed material reference".into()));
                                    }
                                    Some(_) => by_guid = true,
                                }
                            }
                            match name {
                                Some(name) => material_refs.push((mats.len(), name)),
                                None if by_guid => {}
                                None => return Err(O3dError::XFile("missing material reference name".into())),
                            }
                            mats.push(Material::default());
                        }
                        _ => {}
                    }
                }
            }
            _ => skip_object(t, &tok)?,
        }
    }
    if mats.is_empty() {
        mats.push(Material::default());
    }
    for i in 0..nv {
        ctx.mesh.vertices.push(Vertex { position: positions[i], normal: normals[i], uv: uvs[i] });
    }
    for (fi, f) in faces.iter().enumerate() {
        let m = base_m + face_mats[fi].min(mats.len() as u16 - 1);
        for k in 1..f.len().saturating_sub(1) {
            ctx.mesh.triangles.push(Triangle { indices: [base_v + f[0], base_v + f[k], base_v + f[k + 1]], material: m });
        }
    }
    let material_base = ctx.mesh.materials.len();
    ctx.material_refs.extend(
        material_refs
            .into_iter()
            .map(|(slot, name)| (material_base + slot, name)),
    );
    ctx.mesh.materials.extend(mats);
    Ok(())
}

/// D3DDECLUSAGE values the meshes take data from.
const DECLUSAGE_NORMAL: u32 = 3;
const DECLUSAGE_TEXCOORD: u32 = 5;

/// One D3DVERTEXELEMENT9 as DeclData lists it (type, method, usage, usage index; the stream
/// and the offset are implied: the elements follow each other in one stream).
struct VertexElement {
    kind: u32,
    usage: u32,
    usage_index: u32,
}

/// A DeclData object: the vertex layout and the vertex buffer as DWORDs.
struct VertexDeclaration {
    elements: Vec<VertexElement>,
    buffer: Vec<u32>,
}

fn read_vertex_declaration(t: &mut Tok) -> Result<VertexDeclaration, O3dError> {
    let declared = t.bits32()? as usize;
    let mut elements = Vec::with_capacity(declared.min(t.left()));
    while elements.len() < declared {
        let [kind, _method, usage, usage_index] = t.bits32_n::<4>()?;
        elements.push(VertexElement { kind, usage, usage_index });
    }
    let size = t.bits32()? as usize;
    let mut buffer = Vec::with_capacity(size.min(t.left()));
    while buffer.len() < size {
        buffer.push(t.bits32()?);
    }
    Ok(VertexDeclaration { elements, buffer })
}

/// Bytes each D3DDECLTYPE takes in the vertex, FLOAT1 (0) to UNUSED (17).
const DECLTYPE_BYTES: [u8; 18] = [4, 8, 12, 16, 4, 4, 4, 8, 4, 4, 8, 4, 8, 4, 4, 4, 8, 0];

fn element_bytes(kind: u32) -> Option<usize> {
    DECLTYPE_BYTES.get(kind as usize).map(|&b| b as usize)
}

/// A vertex element widened to four floats the way Direct3D 9 does it: missing components
/// are 0, 0, 0, 1; the `N` types are normalised.
fn unpack_element(kind: u32, w: &[u32]) -> Option<Vec4> {
    let f = |i: usize| f32::from_bits(w[i]);
    let halves = |x: u32| [x as u16, (x >> 16) as u16];
    let signed = |x: u32| halves(x).map(|h| h as i16 as f32);
    let unsigned = |x: u32| halves(x).map(f32::from);
    let snorm = |v: f32, max: f32| (v / max).max(-1.0);
    let from_half = |h: u16| half::f16::from_bits(h).to_f32();
    Some(match kind {
        0 => Vec4::new(f(0), 0.0, 0.0, 1.0),
        1 => Vec4::new(f(0), f(1), 0.0, 1.0),
        2 => Vec4::new(f(0), f(1), f(2), 1.0),
        3 => Vec4::new(f(0), f(1), f(2), f(3)),
        // D3DCOLOR: A8R8G8B8, read out as r, g, b, a
        4 => {
            let [b, g, r, a] = w[0].to_le_bytes().map(f32::from);
            Vec4::new(r, g, b, a) / 255.0
        }
        5 => Vec4::from_array(w[0].to_le_bytes().map(f32::from)),
        8 => Vec4::from_array(w[0].to_le_bytes().map(f32::from)) / 255.0,
        6 => {
            let [x, y] = signed(w[0]);
            Vec4::new(x, y, 0.0, 1.0)
        }
        7 => {
            let ([x, y], [z, a]) = (signed(w[0]), signed(w[1]));
            Vec4::new(x, y, z, a)
        }
        9 => {
            let [x, y] = signed(w[0]).map(|v| snorm(v, 32767.0));
            Vec4::new(x, y, 0.0, 1.0)
        }
        10 => {
            let ([x, y], [z, a]) = (signed(w[0]), signed(w[1]));
            Vec4::new(x, y, z, a).map(|v| snorm(v, 32767.0))
        }
        11 => {
            let [x, y] = unsigned(w[0]);
            Vec4::new(x / 65535.0, y / 65535.0, 0.0, 1.0)
        }
        12 => {
            let ([x, y], [z, a]) = (unsigned(w[0]), unsigned(w[1]));
            Vec4::new(x, y, z, a) / 65535.0
        }
        // UDEC3 / DEC3N: three 10-bit fields from bit 0 up
        13 => {
            let field = |shift: u32| ((w[0] >> shift) & 0x3ff) as f32;
            Vec4::new(field(0), field(10), field(20), 1.0)
        }
        14 => {
            let field = |shift: u32| snorm((((w[0] >> shift) << 22) as i32 >> 22) as f32, 511.0);
            Vec4::new(field(0), field(10), field(20), 1.0)
        }
        15 => {
            let [x, y] = halves(w[0]).map(from_half);
            Vec4::new(x, y, 0.0, 1.0)
        }
        16 => {
            let ([x, y], [z, a]) = (halves(w[0]).map(from_half), halves(w[1]).map(from_half));
            Vec4::new(x, y, z, a)
        }
        _ => return None,
    })
}

impl VertexDeclaration {
    /// Take the first normal and the first texture coordinate set out of the buffer. A layout
    /// with a type Direct3D 9 does not have gives no stride, so nothing is taken from it.
    fn apply(&self, to_world: Mat3, normals: &mut [Vec3], uvs: &mut [Vec2]) {
        let mut offsets = Vec::with_capacity(self.elements.len());
        let mut stride = 0;
        for element in &self.elements {
            let Some(bytes) = element_bytes(element.kind) else {
                log::warn!(".x DeclData: unknown vertex element type {}; left out", element.kind);
                return;
            };
            offsets.push(stride);
            stride += bytes / 4;
        }
        if stride == 0 {
            return;
        }
        for (element, &offset) in self.elements.iter().zip(&offsets) {
            if element.usage_index != 0 {
                continue;
            }
            for (v, vertex) in self.buffer.chunks_exact(stride).enumerate() {
                let Some(value) = unpack_element(element.kind, &vertex[offset..]) else {
                    break;
                };
                match element.usage {
                    DECLUSAGE_NORMAL if v < normals.len() => {
                        normals[v] = (to_world * value.truncate()).normalize_or_zero();
                    }
                    DECLUSAGE_TEXCOORD if v < uvs.len() => uvs[v] = Vec2::new(value.x, value.y),
                    _ => {}
                }
            }
        }
    }
}

/// A material declaration after its `Material` token, either inline or a named object.
fn parse_material(t: &mut Tok) -> Result<(Option<String>, Material), O3dError> {
    let name = t.label();
    t.expect("{")?;
    let diffuse = [t.number()?, t.number()?, t.number()?, t.number()?];
    let power = t.number()?;
    let specular = [t.number()?, t.number()?, t.number()?];
    let emissive = [t.number()?, t.number()?, t.number()?];
    let mut texture = String::new();
    loop {
        let Some(tok) = t.next() else {
            return Err(O3dError::XFile("eof in material".into()));
        };
        match tok.key() {
            "}" => break,
            "TextureFilename" => {
                t.enter()?;
                texture = match t.next() {
                    Some(Token::Quoted(file)) => file.into_owned(),
                    Some(other) => other.key().trim_matches('"').to_string(),
                    None => String::new(),
                };
                t.skip_block()?;
            }
            "{" => t.skip_block()?,
            _ => {}
        }
    }
    Ok((
        name,
        Material {
            diffuse,
            specular,
            emissive,
            specular_power: power,
            texture,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    #[ignore = "requires installed traffic-light assets; set OMSI_ROOT or OMSI_TEST_CONTENT"]
    fn installed_traffic_light_material_references() {
        let Some(root) =
            omsi_cfg::flags::OMSI_ROOT.live_os().or_else(|| omsi_cfg::flags::OMSI_TEST_CONTENT.live_os())
        else {
            eprintln!("skipped: set OMSI_ROOT or OMSI_TEST_CONTENT to the installed content root");
            return;
        };
        let object = std::path::PathBuf::from(root).join("Sceneryobjects/D016_semafory");
        let mesh = crate::load_mesh(&object.join("model/semafor_3svetla.X")).unwrap();
        assert!(!mesh.materials.is_empty());
        assert!(!mesh.triangles.is_empty());
        // This housing references PDX01; another globally declared material is unused.
        // Resolving the wrong slot used to leave the housing white.
        for material in &mesh.materials {
            assert!(
                material.texture.eq_ignore_ascii_case("semafor_zaklad.bmp"),
                "unexpected housing texture: {}",
                material.texture
            );
            let texture = omsi_cfg::resolve_path(&object.join("texture"), &material.texture);
            assert!(
                texture.is_file(),
                "missing referenced texture: {}",
                texture.display()
            );
        }
        assert!(mesh
            .triangles
            .iter()
            .all(|triangle| (triangle.material as usize) < mesh.materials.len()));
    }

    fn material_test_mesh(entries: &str) -> String {
        format!(
            "Mesh {{
            3; 0;0;0;, 1;0;0;, 0;1;0;;
            2; 3;0,1,2;, 3;0,2,1;;
            MeshMaterialList {{ 2; 2; 0,1;; {entries} }}
        }}"
        )
    }

    /// Traffic signals exported by 3ds Max put named materials before their frames and
    /// refer to them from each mesh. Ignoring those objects made the housings solid white.
    #[test]
    fn named_materials_keep_textures_colours_and_slot_order() {
        let mesh = material_test_mesh(
            r#"
            { SignalHousing }
            Material { 1;0;0;1;; 2; 0;0;0;; 0;0;0;; }
        "#,
        );
        let x = format!(
            r#"xof 0303txt 0032
            template Material {{ <3d82ab4d-62da-11cf-ab39-0020af71e433> FLOAT ignored; }}
            Material SignalHousing {{
                0.1;0.2;0.3;0.4;; 8; 0.5;0.6;0.7;; 0.2;0.3;0.4;;
                TextureFilename {{ "housing.bmp"; }}
            }}
            Frame Signal {{ {mesh} }}
        "#
        );
        let parsed = parse_x(x.as_bytes()).unwrap();
        assert_eq!(parsed.materials.len(), 2);
        let housing = &parsed.materials[0];
        assert_eq!(housing.texture, "housing.bmp");
        assert_eq!(housing.diffuse, [0.1, 0.2, 0.3, 0.4]);
        assert_eq!(housing.specular, [0.5, 0.6, 0.7]);
        assert_eq!(housing.emissive, [0.2, 0.3, 0.4]);
        assert_eq!(housing.specular_power, 8.0);
        assert_eq!(parsed.materials[1].diffuse, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(
            parsed
                .triangles
                .iter()
                .map(|t| t.material)
                .collect::<Vec<_>>(),
            [0, 1]
        );
    }

    #[test]
    fn material_references_resolve_forward_across_frames_and_inline_definitions() {
        let first = material_test_mesh("{ LaterInFrame } { LaterInline }");
        let second = material_test_mesh(
            r#"
            { LaterInFrame }
            Material LaterInline {
                1;1;1;1;; 1; 0;0;0;; 1;1;1;;
                TextureFilename { "lamp.bmp"; }
            }
        "#,
        );
        let x = format!(
            r#"xof 0303txt 0032
            Frame First {{ {first} }}
            Frame Second {{
                Material LaterInFrame {{
                    1;1;1;1;; 1; 0;0;0;; 0;0;0;;
                    TextureFilename {{ "housing.bmp"; }}
                }}
                Frame Child {{ {second} }}
            }}
        "#
        );
        let parsed = parse_x(x.as_bytes()).unwrap();
        let textures: Vec<&str> = parsed
            .materials
            .iter()
            .map(|m| m.texture.as_str())
            .collect();
        assert_eq!(
            textures,
            ["housing.bmp", "lamp.bmp", "housing.bmp", "lamp.bmp"]
        );
        assert_eq!(
            parsed
                .triangles
                .iter()
                .map(|t| t.material)
                .collect::<Vec<_>>(),
            [0, 1, 2, 3]
        );
    }

    #[test]
    fn missing_material_reference_preserves_following_slots() {
        let mesh = material_test_mesh(
            r#"
            { NotDeclared }
            Material { 1;0;0;1;; 1; 0;0;0;; 0;0;0;; }
        "#,
        );
        let parsed = parse_x(format!("xof 0303txt 0032\n{mesh}").as_bytes()).unwrap();
        assert_eq!(parsed.materials[0], Material::default());
        assert_eq!(parsed.materials[1].diffuse, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(
            parsed
                .triangles
                .iter()
                .map(|t| t.material)
                .collect::<Vec<_>>(),
            [0, 1]
        );
    }

    /// A Blender export: the root frame swaps Y and Z, the child frame scales, turns about
    /// its up axis and moves the mesh 5.5 m forward and 1.3 m up (the Atron terminal's frames).
    #[test]
    fn frame_matrices_are_row_major() {
        let x = "xof 0303txt 0032\n\
Frame Root {\n FrameTransformMatrix {\n 1.0, 0.0, 0.0, 0.0,\n 0.0,-0.0, 1.0, 0.0,\n 0.0, 1.0, 0.0, 0.0,\n 0.0, 0.0, 0.0, 1.0;;\n }\n\
 Frame Child {\n  FrameTransformMatrix {\n   0.0, 2.0, 0.0, 0.0,\n  -2.0, 0.0, 0.0, 0.0,\n   0.0, 0.0, 2.0, 0.0,\n  -0.5, 5.5, 1.3, 1.0;;\n  }\n\
  Mesh {\n   3;\n   1.0;0.0;0.0;,\n   0.0;1.0;0.0;,\n   0.0;0.0;1.0;;\n   1;\n   3;0,1,2;;\n\
   MeshNormals {\n    1;\n    1.0;0.0;0.0;;\n    1;\n    3;0,0,0;;\n   }\n  }\n }\n}\n";
        let m = parse_x(x.as_bytes()).unwrap();
        let p: Vec<Vec3> = m.vertices.iter().map(|v| v.position).collect();
        // Blender (x, y, z) -> child: (x, y, z)·M = (-2y, 2x, 2z) + (-0.5, 5.5, 1.3), then the
        // root swaps y and z.
        let want = [Vec3::new(-0.5, 1.3, 7.5), Vec3::new(-2.5, 1.3, 5.5), Vec3::new(-0.5, 3.3, 5.5)];
        for (a, b) in p.iter().zip(want) {
            assert!((*a - b).length() < 1e-5, "{p:?}");
        }
        // the +x normal turns to Blender +y, D3D +z
        assert!((m.vertices[0].normal - Vec3::Z).length() < 1e-5, "{:?}", m.vertices[0].normal);
    }

    #[test]
    fn names_with_accented_letters_load() {
        let x = b"xof 0303txt 0032\nFrame \x8Aasi\xA0\x85 {\n Material \x8Aed\xE1 { 1;1;1;1;; 1; 0;0;0;; 0;0;0;; }\n Mesh {\n 3;\n 0;0;0;,\n 1;0;0;,\n 0;1;0;;\n 1;\n 3;0,1,2;;\n }\n}\n";
        assert_eq!(parse_x(x).unwrap().triangles.len(), 1);
    }

    #[test]
    fn frames_nested_too_deep_are_an_error() {
        let deep = |n: usize| {
            let mut x = String::from("xof 0303txt 0032\n");
            x.push_str(&"Frame{".repeat(n));
            x.push_str("Mesh{3;0;0;0;,1;0;0;,0;1;0;;1;3;0,1,2;;}");
            x.push_str(&"}".repeat(n));
            x
        };
        assert_eq!(parse_x(deep(MAX_FRAME_DEPTH).as_bytes()).unwrap().triangles.len(), 1);
        assert!(parse_x(deep(MAX_FRAME_DEPTH + 1).as_bytes()).is_err());
        assert!(parse_x(deep(200_000).as_bytes()).is_err());
    }

    /// A Blender export: the root frame swaps y and z, the child frame turns, scales and
    /// moves the mesh (Direct3D row-vector matrices).
    #[test]
    fn frames_move_and_turn_like_direct3d() {
        let x = "xof 0303txt 0032\nFrame Root {\n FrameTransformMatrix {\n 1,0,0,0,\n 0,0,1,0,\n 0,1,0,0,\n 0,0,0,1;;\n }\n Frame Box {\n FrameTransformMatrix {\n 2,0,0,0,\n 0,0,2,0,\n 0,-2,0,0,\n 0.5,0.25,1.5,1;;\n }\n Mesh {\n 3;\n 0;0;0;,\n 1;0;0;,\n 0;1;0;;\n 1;\n 3;0,1,2;;\n }\n }\n}\n";
        let m = parse_x(x.as_bytes()).unwrap();
        let p: Vec<Vec3> = m.vertices.iter().map(|v| v.position).collect();
        // v * child: (x, y, z) -> (2x + 0.5, -2z + 0.25, 2y + 1.5); then y and z swap
        assert!((p[0] - Vec3::new(0.5, 1.5, 0.25)).length() < 1e-5, "{:?}", p[0]);
        assert!((p[1] - Vec3::new(2.5, 1.5, 0.25)).length() < 1e-5, "{:?}", p[1]);
        assert!((p[2] - Vec3::new(0.5, 3.5, 0.25)).length() < 1e-5, "{:?}", p[2]);
    }

    /// A frame that scales unevenly (the Ruede Trafohaus: 1.5 x 0.75 x 0.85) keeps the
    /// normals on their faces: they go by the inverse transpose (the frame matrix itself put
    /// the transformer house's normals 3.8° off its walls).
    #[test]
    fn uneven_scale_keeps_normals_on_their_faces() {
        let x = "xof 0303txt 0032\nFrame Box {\n FrameTransformMatrix {\n 3,0,0,0,\n 0,1,0,0,\n 0,0,1,0,\n 0,0,0,1;;\n }\n Mesh {\n 3;\n 1;0;0;,\n 0;1;0;,\n 0;0;1;;\n 1;\n 3;0,1,2;;\n MeshNormals {\n 1;\n 0.57735;0.57735;0.57735;;\n 1;\n 3;0,0,0;;\n }\n }\n}\n";
        let m = parse_x(x.as_bytes()).unwrap();
        let [a, b, c] = [0, 1, 2].map(|i| m.vertices[i].position);
        let face = (b - a).cross(c - a).normalize();
        for v in &m.vertices {
            assert!(v.normal.normalize().dot(face).abs() > 0.9999, "{:?} vs {face:?}", v.normal);
        }
    }
}

#[cfg(test)]
mod damaged_tests {
    use super::*;

    #[test]
    fn damaged_files_are_errors_not_panics() {
        assert!(parse_x(b"xof 03").is_err());
        assert!(parse_x(b"xof 0302\xe9\xe9\xe9\xe9 0032 Mesh {").is_err());
        // a face naming vertex 7 of 3
        let m = parse_x(b"xof 0302txt 0032\nMesh m {\n3;\n0;0;0;,\n1;0;0;,\n0;1;0;;\n2;\n3;0,1,2;,\n3;0,1,7;;\n}\n").unwrap();
        assert_eq!(m.triangles.len(), 1);
    }
}

#[cfg(test)]
mod binary_tests {
    use super::*;

    /// Writes binary .x tokens the way the DirectX file format lays them out.
    struct XBin {
        buf: Vec<u8>,
        wide: bool,
    }

    impl XBin {
        fn new(wide: bool) -> Self {
            let header: &[u8] = if wide { b"xof 0303bin 0064" } else { b"xof 0303bin 0032" };
            XBin { buf: header.to_vec(), wide }
        }
        fn token(&mut self, id: u16) -> &mut Self {
            self.buf.extend_from_slice(&id.to_le_bytes());
            self
        }
        fn u32le(&mut self, v: u32) -> &mut Self {
            self.buf.extend_from_slice(&v.to_le_bytes());
            self
        }
        fn open(&mut self) -> &mut Self {
            self.token(10)
        }
        fn close(&mut self) -> &mut Self {
            self.token(11)
        }
        fn name(&mut self, w: &str) -> &mut Self {
            self.token(1).u32le(w.len() as u32);
            self.buf.extend_from_slice(w.as_bytes());
            self
        }
        fn string(&mut self, s: &str) -> &mut Self {
            self.token(2).u32le(s.len() as u32);
            self.buf.extend_from_slice(s.as_bytes());
            self.u32le(20)
        }
        fn int_list(&mut self, v: &[u32]) -> &mut Self {
            self.token(6).u32le(v.len() as u32);
            v.iter().for_each(|&x| {
                self.u32le(x);
            });
            self
        }
        fn float_list(&mut self, v: &[f64]) -> &mut Self {
            self.token(7).u32le(v.len() as u32);
            for &x in v {
                if self.wide {
                    self.buf.extend_from_slice(&x.to_le_bytes());
                } else {
                    self.buf.extend_from_slice(&(x as f32).to_le_bytes());
                }
            }
            self
        }
    }

    /// A 2 m x 3 m roof panel moved 4 m along x by its frame, with a texture on a named
    /// material defined at the top of the file and a template the reader skips.
    fn roof_panel(wide: bool) -> Vec<u8> {
        let mut x = XBin::new(wide);
        x.token(31).name("Mesh").open().token(5);
        x.buf.extend_from_slice(&[0x11; 16]);
        x.token(41).name("nVertices").token(20).close();
        x.name("Material").name("RoofTiles").open();
        x.float_list(&[0.8, 0.3, 0.2, 1.0, 12.0, 0.1, 0.1, 0.1, 0.0, 0.0, 0.0]);
        x.name("TextureFilename").open().string("ziegel_braun.tga").close();
        x.close();
        x.name("Frame").name("Roof").open();
        x.name("FrameTransformMatrix").open();
        x.float_list(&[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 4.0, 0.0, 0.0, 1.0]);
        x.close();
        x.name("Mesh").name("Panel").open();
        x.int_list(&[4]).float_list(&[0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 2.0, 3.0, 0.0, 0.0, 3.0, 0.0]);
        x.int_list(&[1, 4, 0, 1, 2, 3]);
        x.name("MeshTextureCoords").open().int_list(&[4]).float_list(&[0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0]).close();
        x.name("MeshMaterialList").open().int_list(&[1, 1, 0]);
        x.open().name("RoofTiles").close();
        x.close();
        x.close().close();
        x.buf
    }

    #[test]
    fn binary_roof_panel_loads_with_its_frame_texture_and_uvs() {
        for wide in [false, true] {
            let m = parse_x(&roof_panel(wide)).unwrap();
            assert_eq!(m.triangles.len(), 2, "wide {wide}");
            assert_eq!(m.vertices[2].position, Vec3::new(6.0, 3.0, 0.0));
            let uvs: Vec<[f32; 2]> = m.vertices.iter().map(|v| v.uv.to_array()).collect();
            assert_eq!(uvs, [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
            assert_eq!(m.materials.len(), 1);
            let roof = &m.materials[0];
            assert!(roof.texture == "ziegel_braun.tga", "{roof:?}");
            assert!((roof.diffuse[0] - 0.8).abs() < 1e-6 && roof.specular_power == 12.0);
        }
    }

    #[test]
    fn binary_and_text_of_one_panel_agree() {
        let text = "xof 0303txt 0032\n\
            Material RoofTiles { 0.8;0.3;0.2;1.0;; 12.0; 0.1;0.1;0.1;; 0.0;0.0;0.0;;\n\
              TextureFilename { \"ziegel_braun.tga\"; } }\n\
            Frame Roof { FrameTransformMatrix { 1,0,0,0, 0,1,0,0, 0,0,1,0, 4,0,0,1;; }\n\
              Mesh Panel { 4; 0;0;0;, 2;0;0;, 2;3;0;, 0;3;0;; 1; 4;0,1,2,3;;\n\
                MeshTextureCoords { 4; 0;1;, 1;1;, 1;0;, 0;0;; }\n\
                MeshMaterialList { 1; 1; 0;; { RoofTiles } } } }\n";
        let (a, b) = (parse_x(text.as_bytes()).unwrap(), parse_x(&roof_panel(false)).unwrap());
        assert_eq!(a.vertices, b.vertices);
        assert_eq!(a.triangles, b.triangles);
        assert_eq!(a.materials, b.materials);
    }

    /// Exporters that write a vertex declaration put the normals and texture coordinates in
    /// DeclData instead of MeshNormals / MeshTextureCoords: without it the mesh is one colour.
    #[test]
    fn vertex_declaration_supplies_normals_and_uvs() {
        let f = |v: f32| v.to_bits().to_string();
        let vertex = |n: [f32; 3], uv: [f32; 2]| {
            [n[0], n[1], n[2], uv[0], uv[1], 9.0, 9.0].map(f).join(",")
        };
        let data = [
            vertex([0.0, 0.0, 1.0], [0.25, 0.75]),
            vertex([0.0, 1.0, 0.0], [0.5, 0.5]),
            vertex([1.0, 0.0, 0.0], [1.0, 0.0]),
        ]
        .join(",");
        // elements: FLOAT3 normal, FLOAT2 texcoord 0, FLOAT2 texcoord 1 (a light map, unused)
        let x = format!(
            "xof 0303txt 0032\nMesh Tree {{ 3; 0;0;0;, 1;0;0;, 0;1;0;; 1; 3;0,1,2;;\n\
             DeclData {{ 3; 2;0;3;0;, 1;0;5;0;, 1;0;5;1;; 21; {data};\n }} }}\n"
        );
        let m = parse_x(x.as_bytes()).unwrap();
        let got: Vec<(Vec3, Vec2)> = m.vertices.iter().map(|v| (v.normal, v.uv)).collect();
        assert_eq!(
            got,
            [(Vec3::Z, Vec2::new(0.25, 0.75)), (Vec3::Y, Vec2::new(0.5, 0.5)), (Vec3::X, Vec2::new(1.0, 0.0))]
        );
    }

    #[test]
    fn binary_vertex_declaration_with_packed_types() {
        let mut x = XBin::new(false);
        x.name("Mesh").open();
        x.int_list(&[2]).float_list(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        x.int_list(&[1, 3, 0, 1, 1]);
        // DEC3N normal, D3DCOLOR diffuse (not used), FLOAT16_2 texcoord 0
        x.name("DeclData").open().int_list(&[3, 14, 0, 3, 0, 4, 0, 10, 0, 15, 0, 5, 0]);
        let up = 511 << 20;
        let minus_x = 0x201;
        let uv_a = 0x3800 | (0x3400 << 16); // 0.5, 0.25
        let uv_b = 0x3c00; // 1, 0
        x.int_list(&[6, up, 0xff00_00ff, uv_a, minus_x, 0xffff_ffff, uv_b]);
        x.close().close();
        let [a, b] = [0, 1].map(|i| parse_x(&x.buf).unwrap().vertices[i]);
        assert!(a.normal.abs_diff_eq(Vec3::Z, 1e-6) && b.normal.abs_diff_eq(-Vec3::X, 1e-6), "{a:?} {b:?}");
        assert_eq!((a.uv, b.uv), (Vec2::new(0.5, 0.25), Vec2::X));
    }

    #[test]
    fn packed_element_types_widen_like_direct3d() {
        let near = |a: Vec4, b: Vec4| (a - b).length() < 1e-4;
        // SHORT2N: -32768 clamps to -1
        assert!(near(unpack_element(9, &[0x8000_7fff]).unwrap(), Vec4::new(1.0, -1.0, 0.0, 1.0)));
        // UBYTE4N
        assert!(near(unpack_element(8, &[0x00ff_33ff]).unwrap(), Vec4::new(1.0, 0.2, 1.0, 0.0)));
        // D3DCOLOR is stored as ARGB
        assert!(near(unpack_element(4, &[0x80ff_0000]).unwrap(), Vec4::new(1.0, 0.0, 0.0, 128.0 / 255.0)));
        // USHORT2N
        assert!(near(unpack_element(11, &[0xffff_0000]).unwrap(), Vec4::new(0.0, 1.0, 0.0, 1.0)));
        // UDEC3
        assert!(near(unpack_element(13, &[3 | (2 << 10) | (1 << 20)]).unwrap(), Vec4::new(3.0, 2.0, 1.0, 1.0)));
        // SHORT4 keeps its integers
        assert!(near(unpack_element(7, &[0xfffe_0005, 0x0001_0000]).unwrap(), Vec4::new(5.0, -2.0, 0.0, 1.0)));
        assert_eq!(unpack_element(99, &[0]), None);
        assert_eq!(element_bytes(17), Some(0));
    }

    #[test]
    fn unknown_vertex_element_types_leave_the_mesh_as_it_was() {
        let x = "xof 0303txt 0032\nMesh { 1; 0;0;0;; 0;;\n DeclData { 1; 42;0;3;0;; 1; 7; } }\n";
        let m = parse_x(x.as_bytes()).unwrap();
        assert_eq!(m.vertices[0].normal, Vec3::Z);
    }

    /// Objects and references may carry a GUID after their name, or only a GUID.
    #[test]
    fn binary_guids_after_names_and_in_references() {
        let mut x = XBin::new(false);
        let guid = |x: &mut XBin| {
            x.token(5).buf.extend_from_slice(&[0x5a; 16]);
        };
        x.name("Material").name("Glass");
        guid(&mut x);
        x.open().float_list(&[0.2, 0.4, 0.6, 0.5, 30.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0]).close();
        x.name("Frame").name("Window");
        guid(&mut x);
        x.open().name("Mesh");
        guid(&mut x);
        x.open();
        x.int_list(&[3]).float_list(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
        x.int_list(&[2, 3, 0, 1, 2, 3, 0, 2, 1]);
        x.name("MeshMaterialList").open().int_list(&[2, 2, 0, 1]);
        x.open();
        guid(&mut x);
        x.close();
        x.open().name("Glass");
        guid(&mut x);
        x.close();
        x.close().close().close();
        let m = parse_x(&x.buf).unwrap();
        assert_eq!(m.triangles.len(), 2);
        assert_eq!(m.materials.len(), 2);
        assert_eq!(m.materials[0], Material::default());
        assert_eq!(m.materials[1].diffuse, [0.2, 0.4, 0.6, 0.5]);
    }

    #[test]
    fn cut_or_broken_binary_files_are_errors() {
        let whole = roof_panel(false);
        for len in 16..whole.len() - 4 {
            let _ = parse_x(&whole[..len]);
        }
        let cut_short = &whole[..whole.len() - 6];
        assert!(parse_x(cut_short).is_err());
        let mut odd = XBin::new(false);
        odd.name("Mesh").open().token(77);
        assert!(parse_x(&odd.buf).is_err());
        let mut huge = XBin::new(false);
        huge.name("Mesh").open().token(7).u32le(u32::MAX);
        assert!(parse_x(&huge.buf).is_err());
        assert!(parse_x(b"xof 0303tzip0032\0\0\0\0").is_err());
    }

    /// Every binary .x under the content root loads and has faces (the PAD-Labs houses are
    /// binary files with DeclData).
    #[test]
    #[ignore = "requires installed content; set OMSI_TEST_CONTENT or OMSI_ROOT"]
    fn installed_binary_x_files_load() {
        let Some(root) =
            omsi_cfg::flags::OMSI_TEST_CONTENT.live_os().or_else(|| omsi_cfg::flags::OMSI_ROOT.live_os())
        else {
            eprintln!("skipped: set OMSI_TEST_CONTENT or OMSI_ROOT to the installed content root");
            return;
        };
        fn walk(dir: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else { return };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, found);
                } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("x")) {
                    found.push(path);
                }
            }
        }
        let mut files = Vec::new();
        walk(&std::path::PathBuf::from(root).join("Sceneryobjects"), &mut files);
        let mut binary = 0;
        for file in files {
            let bytes = std::fs::read(&file).unwrap();
            if bytes.get(8..12) != Some(b"bin ") {
                continue;
            }
            binary += 1;
            let mesh = parse_x(&bytes).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
            assert!(!mesh.triangles.is_empty(), "{} has no faces", file.display());
        }
        eprintln!("{binary} binary .x files loaded");
    }
}
