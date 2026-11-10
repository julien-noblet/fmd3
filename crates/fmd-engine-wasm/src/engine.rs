use crate::abi::*;
use anyhow::{anyhow, Context, Result};
use std::path::Path;
use tracing::{error, info, warn};
use wasmtime::*;

pub struct HostState {
    pub last_error: Option<String>,
}

pub struct WasmEngine {
    engine: Engine,
}

impl WasmEngine {
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.consume_fuel(true);

        let engine = Engine::new(&config)?;
        Ok(Self { engine })
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Load and instantiate a WASM module from a file or buffer
    pub fn load_module(&self, wasm_bytes: &[u8]) -> Result<WasmInstance> {
        let module = Module::new(&self.engine, wasm_bytes)
            .map_err(|e| anyhow!("Échec de la compilation du module WebAssembly: {}", e))?;

        let mut linker = Linker::new(&self.engine);

        // Host Function: host_log(level: i32, ptr: i32, len: i32)
        linker.func_wrap(
            "env",
            "host_log",
            |mut caller: Caller<'_, HostState>, level: i32, ptr: i32, len: i32| {
                let mem = match caller.get_export("memory").and_then(|e| e.into_memory()) {
                    Some(m) => m,
                    None => return,
                };
                let data = mem.data(&caller);
                if ptr < 0 || (ptr as usize + len as usize) > data.len() {
                    return;
                }
                let slice = &data[ptr as usize..(ptr as usize + len as usize)];
                let msg = String::from_utf8_lossy(slice);
                match level {
                    1 => error!("[WASM Guest] {}", msg),
                    2 => warn!("[WASM Guest] {}", msg),
                    _ => info!("[WASM Guest] {}", msg),
                }
            },
        )?;

        let state = HostState { last_error: None };
        let mut store = Store::new(&self.engine, state);
        // Add 10,000,000 fuel units per execution
        store.set_fuel(10_000_000)?;

        let instance = linker.instantiate(&mut store, &module)?;

        Ok(WasmInstance { store, instance })
    }

    pub fn load_module_from_file<P: AsRef<Path>>(&self, path: P) -> Result<WasmInstance> {
        let bytes = std::fs::read(path.as_ref())
            .with_context(|| format!("Impossible de lire le fichier WASM {:?}", path.as_ref()))?;
        self.load_module(&bytes)
    }
}

pub struct WasmInstance {
    store: Store<HostState>,
    instance: Instance,
}

impl WasmInstance {
    /// Helper to allocate memory in the WASM guest if `_fmd_alloc` is exported
    pub fn alloc(&mut self, size: usize) -> Result<i32> {
        if let Ok(alloc_fn) = self
            .instance
            .get_typed_func::<i32, i32>(&mut self.store, "_fmd_alloc")
        {
            let ptr = alloc_fn.call(&mut self.store, size as i32)?;
            Ok(ptr)
        } else {
            // Default offset 0 if custom allocator is not present
            Ok(0)
        }
    }

    /// Read bytes from guest memory
    pub fn read_bytes(&mut self, ptr: i32, len: usize) -> Result<Vec<u8>> {
        let memory = self
            .instance
            .get_memory(&mut self.store, "memory")
            .ok_or_else(|| anyhow!("Export 'memory' introuvable dans le module WASM"))?;

        let data = memory.data(&self.store);
        let start = ptr as usize;
        let end = start + len;
        if end > data.len() {
            return Err(anyhow!(
                "Dépassement mémoire lors de la lecture (ptr={}, len={}, total={})",
                ptr,
                len,
                data.len()
            ));
        }
        Ok(data[start..end].to_vec())
    }

    /// Write bytes into guest memory
    pub fn write_bytes(&mut self, ptr: i32, bytes: &[u8]) -> Result<()> {
        let memory = self
            .instance
            .get_memory(&mut self.store, "memory")
            .ok_or_else(|| anyhow!("Export 'memory' introuvable dans le module WASM"))?;

        let data = memory.data_mut(&mut self.store);
        let start = ptr as usize;
        let end = start + bytes.len();
        if end > data.len() {
            return Err(anyhow!(
                "Dépassement mémoire lors de l'écriture (ptr={}, len={}, total={})",
                ptr,
                bytes.len(),
                data.len()
            ));
        }
        data[start..end].copy_from_slice(bytes);
        Ok(())
    }

    /// Call a generic JSON-based ABI function: Input -> Output
    pub fn call_json<I: serde::Serialize, O: serde::de::DeserializeOwned>(
        &mut self,
        func_name: &str,
        input: &I,
    ) -> Result<O> {
        // Refill fuel for each call
        let _ = self.store.set_fuel(10_000_000);

        let input_bytes = serde_json::to_vec(input)?;
        let ptr = self.alloc(input_bytes.len())?;
        self.write_bytes(ptr, &input_bytes)?;

        let typed_func = self
            .instance
            .get_typed_func::<(i32, i32), i64>(&mut self.store, func_name)
            .map_err(|e| anyhow!("Fonction exportée '{}' introuvable: {}", func_name, e))?;

        let ret = typed_func.call(&mut self.store, (ptr, input_bytes.len() as i32))?;

        let out_ptr = (ret >> 32) as i32;
        let out_len = (ret & 0xFFFF_FFFF) as usize;

        let out_bytes = self.read_bytes(out_ptr, out_len)?;
        let result: O = serde_json::from_slice(&out_bytes).with_context(|| {
            format!(
                "Échec de désérialisation de la réponse JSON de '{}'",
                func_name
            )
        })?;

        Ok(result)
    }

    /// Retrieve module metadata
    pub fn get_metadata(&mut self) -> Result<WasmModuleMeta> {
        let _ = self.store.set_fuel(5_000_000);
        let typed_func = self
            .instance
            .get_typed_func::<(), i64>(&mut self.store, "_fmd_meta")
            .map_err(|e| anyhow!("Fonction '_fmd_meta' introuvable: {}", e))?;

        let ret = typed_func.call(&mut self.store, ())?;
        let out_ptr = (ret >> 32) as i32;
        let out_len = (ret & 0xFFFF_FFFF) as usize;

        let out_bytes = self.read_bytes(out_ptr, out_len)?;
        let meta: WasmModuleMeta = serde_json::from_slice(&out_bytes)?;
        Ok(meta)
    }

    /// Process image through WASM filter
    pub fn process_image(
        &mut self,
        image_bytes: &[u8],
        _filename: &str,
        _format: &str,
    ) -> Result<Vec<u8>> {
        let _ = self.store.set_fuel(20_000_000);

        let ptr = self.alloc(image_bytes.len())?;
        self.write_bytes(ptr, image_bytes)?;

        let typed_func = self
            .instance
            .get_typed_func::<(i32, i32), i64>(&mut self.store, "_fmd_process_image")
            .map_err(|e| anyhow!("Fonction '_fmd_process_image' introuvable: {}", e))?;

        let ret = typed_func.call(&mut self.store, (ptr, image_bytes.len() as i32))?;
        let out_ptr = (ret >> 32) as i32;
        let out_len = (ret & 0xFFFF_FFFF) as usize;

        let out_bytes = self.read_bytes(out_ptr, out_len)?;
        Ok(out_bytes)
    }
}
