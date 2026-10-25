use crate::render::pipeline::blob_bytes;
use anyhow::anyhow;
use rapidhash::v3::rapidhash_v3;
use serde::{Deserialize, Serialize};
use skip_bom::{BomType, SkipEncodingBom};
use std::collections::HashMap;
use std::ffi::c_void;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use windows::core::{implement, w, Interface, PCWSTR};
use windows::Win32::Foundation::E_FAIL;
use windows::Win32::Graphics::Direct3D::Dxc::{CLSID_DxcCompiler, CLSID_DxcUtils, DxcBuffer, DxcCreateInstance, IDxcBlob, IDxcCompiler3, IDxcIncludeHandler, IDxcIncludeHandler_Impl, IDxcResult, IDxcUtils, DXC_CP_UTF8};
use windows::Win32::Graphics::Direct3D::ID3DBlob;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ShaderMetadata {
    pub targets: Vec<ShaderTarget>
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ShaderTarget {
    Fragment {
        entry: String,
    },
    Vertex {
        entry: String,
    },
    Domain {
        entry: String,
    },
    Hull {
        entry: String
    },
    Compute {
        entry: String
    },
    Geometry {
        entry: String
    }
}

impl ShaderTarget {
    pub fn get_type(&self) -> ShaderType {
        match self {
            ShaderTarget::Fragment { .. } => ShaderType::Fragment,
            ShaderTarget::Vertex { .. } => ShaderType::Vertex,
            ShaderTarget::Domain { .. } => ShaderType::Domain,
            ShaderTarget::Hull { .. } => ShaderType::Hull,
            ShaderTarget::Compute { .. } => ShaderType::Compute,
            ShaderTarget::Geometry { .. } => ShaderType::Geometry
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShaderType {
    Compute,
    Vertex,
    Hull,
    Domain,
    Geometry,
    Fragment,
}

#[derive(Debug, Clone, Default)]
struct ShaderPair {
    pub hash: u64,
    pub compiled_blobs: HashMap<ShaderType, ID3DBlob>,
    pub metadata: ShaderMetadata,
}

#[derive(Debug)]
pub struct ShaderStorage {
    root_dir: PathBuf,
    lib_shaders: HashMap<PathBuf, String>,
    lib_hashes: HashMap<PathBuf, u64>,
    shaders: HashMap<PathBuf, ShaderPair>,
    staging_shaders: HashMap<PathBuf, ShaderPair>,
    dirty_files: Vec<PathBuf>,
    compiler: IDxcCompiler3,
    utils: IDxcUtils
}

impl ShaderStorage {
    pub fn new<P: AsRef<Path>>(root_dir: P) -> anyhow::Result<Self> {
        let compiler: IDxcCompiler3 = unsafe { DxcCreateInstance(&CLSID_DxcCompiler)? };
        let utils: IDxcUtils = unsafe { DxcCreateInstance(&CLSID_DxcUtils)? };
        Ok(Self {
            root_dir: root_dir.as_ref().to_path_buf(),
            lib_shaders: HashMap::new(),
            lib_hashes: HashMap::new(),
            shaders: HashMap::new(),
            staging_shaders: HashMap::new(),
            dirty_files: vec![],
            compiler,
            utils
        })
    }

    pub fn recompile(&mut self) -> anyhow::Result<()> {
        self.staging_shaders = self.shaders.clone();
        if let Err(err) = self.compile() {
            self.shaders = self.staging_shaders.clone();
            self.staging_shaders.clear();
            Err(err)
        } else {
            self.staging_shaders.clear();
            Ok(())
        }
    }

    pub fn compile(&mut self) -> anyhow::Result<()> {
        self.lib_shaders.clear();
        self.dirty_files.clear();

        let mut files = HashMap::new();
        let mut all_dirty = false;
        let mut new_lib_hashes = HashMap::new();
        // first sweep: collecting all ./lib shaders
        for child in WalkDir::new(&self.root_dir.join("lib"))
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file()) {
            let buf = child.path().to_path_buf();
            let mut str = String::new();
            let mut reader = SkipEncodingBom::new(BomType::all(), File::open(&buf)?);
            reader.read_to_string(&mut str)?;
            let new_hash = rapidhash_v3(&str.as_bytes());
            if let Some(old_hash) = self.lib_hashes.get(&buf) {
                if *old_hash != new_hash {
                    all_dirty = true;
                }
            } else {
                all_dirty = true;
            }

            self.lib_shaders.insert(buf.clone(), str);
            new_lib_hashes.insert(buf, new_hash);
        }

        // second sweep: marking dirty files and collecting a file map
        for child in WalkDir::new(&self.root_dir)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e|
                e.file_type().is_file()
                    && !e.path().parent().unwrap().to_str().unwrap().ends_with("lib")
                    && !e.file_name().to_str().unwrap().ends_with(".json")
            ) // excluding lib shaders here and json meta files
        {
            let path = child.path();
            let pathbuf = PathBuf::from(path);
            let file = File::open(&path)?;
            let mut bytes = Vec::new();
            let mut reader = SkipEncodingBom::new(BomType::all(), &file);
            reader.read_to_end(&mut bytes)?;
            self.check_dirty(&pathbuf, &bytes);
            files.insert(pathbuf, bytes);
        }

        // third sweep: actually compiling shaders
        for (path, bytes) in files {
            self.compile_single(path, bytes, all_dirty)?;
        }

        self.lib_hashes = new_lib_hashes;

        Ok(())
    }

    pub fn get_shader<P: AsRef<Path>>(&self, rel_path: P, ty: ShaderType) -> Option<&ID3DBlob> {
        let path = rel_path.as_ref();
        let path = self.root_dir.join(path);
        self.shaders.get(&path)
            .map(|pair| pair.compiled_blobs.get(&ty))
            .flatten()
    }

    fn check_dirty(&mut self, path: &PathBuf, bytes: &Vec<u8>) {
        let hash = rapidhash_v3(&bytes);
        if let Some(old_shader) = self.shaders.get(path) {
            if old_shader.hash != hash {
                // have it compiled, dont have the newest version. recompile
                self.dirty_files.push(path.clone());
                self.shaders.insert(path.clone(), ShaderPair {
                    hash,
                    compiled_blobs: HashMap::new(),
                    metadata: ShaderMetadata::default(),
                });
            }
            // have it compiled and its fine. dont recompile
        } else {
            // don't have it. compile from anew
            self.dirty_files.push(path.clone());
            self.shaders.insert(path.clone(), ShaderPair {
                hash,
                compiled_blobs: HashMap::new(),
                metadata: ShaderMetadata::default(),
            });
        }
    }

    fn compile_single(&mut self, path: PathBuf, bytes: Vec<u8>, force: bool) -> anyhow::Result<()> {
        // if we are clean, ignoring
        if !self.dirty_files.contains(&path) && !force {
            return Ok(())
        }

        // reading metadata
        let file_name = path.file_name().unwrap().to_str().unwrap().split('.').next().unwrap();
        let meta = File::open(path.parent().unwrap().join(format!("{file_name}.json"))).map_err(|e| anyhow!("Failed to read metadata for {file_name}: {e}"))?;
        let meta: ShaderMetadata = serde_json::from_reader(SkipEncodingBom::new(BomType::all(), &meta))?;
        let mut blob_map = HashMap::with_capacity(meta.targets.len());
        let include: IDxcIncludeHandler = LibIncludes {
            root_dir: self.root_dir.clone(),
            lib_shaders: self.lib_shaders.clone(),
            utils: self.utils.clone(),
        }.into();
        for target in &meta.targets {
            // compile for each target
            let hstr = path.file_name().unwrap().to_str().unwrap().encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
            let (entry, target_name) = match &target {
                ShaderTarget::Fragment { entry } => (entry, w!("ps_6_6")),
                ShaderTarget::Vertex { entry } => (entry, w!("vs_6_6")),
                ShaderTarget::Domain { entry } => (entry, w!("ds_6_6")),
                ShaderTarget::Hull { entry } => (entry, w!("hs_6_6")),
                ShaderTarget::Compute { entry } => (entry, w!("cs_6_6")),
                ShaderTarget::Geometry { entry } => (entry, w!("gs_6_6"))
            };
            let entry = entry.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();

            let r: IDxcResult = unsafe {
                let src = DxcBuffer { Ptr: bytes.as_ptr().cast(), Size: bytes.len(), Encoding: DXC_CP_UTF8.0 };
                self.compiler.Compile(
                    &src as *const DxcBuffer,
                    Some(&[
                        PCWSTR(hstr.as_ptr()),
                        w!("-E"),
                        PCWSTR(entry.as_ptr()),
                        w!("-T"),
                        target_name,
                        #[cfg(debug_assertions)]
                        w!("-Zi"),
                        #[cfg(debug_assertions)]
                        w!("-Od")
                    ]),
                    &include,
                )?
            };
            if unsafe { r.GetStatus()? }.is_err() {
                let e = unsafe { r.GetErrorBuffer()? };
                let cast: ID3DBlob = e.cast()?;
                let msg = String::from_utf8_lossy(blob_bytes(&cast));
                anyhow::bail!("Failed to compile shader at {}: {msg}", path.display());
            }
            let blob: ID3DBlob = unsafe { r.GetResult()? }.cast()?;
            blob_map.insert(target.get_type(), blob);
        }

        let shader = self.shaders.get_mut(&path).unwrap();
        shader.compiled_blobs = blob_map;
        shader.metadata = meta;

        Ok(())
    }
}

#[implement(IDxcIncludeHandler)]
struct LibIncludes {
    root_dir: PathBuf,
    lib_shaders: HashMap<PathBuf, String>,
    utils: IDxcUtils,
}

#[allow(non_snake_case)]
impl IDxcIncludeHandler_Impl for LibIncludes_Impl {
    fn LoadSource(&self, pfilename: &PCWSTR) -> windows::core::Result<IDxcBlob> {
        let name =
            unsafe { pfilename.to_string() }.map_err(|_| windows::core::Error::from(E_FAIL))?;
        let name = name.replace('\\', "/");
        let key = name.trim_start_matches("./").trim_start_matches("../");
        let path = self.root_dir.join(key);

        let src = self
            .lib_shaders
            .get(&path)
            .ok_or_else(|| windows::core::Error::from(E_FAIL))?;
        let blob = unsafe {
            let ppdata = src.as_str().as_ptr() as *mut c_void;
            let pbytes = src.len() as u32;
            self.utils.CreateBlob(ppdata, pbytes, DXC_CP_UTF8)?
        };
        blob.cast::<IDxcBlob>()
    }
}