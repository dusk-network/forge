#[cfg(feature = "schema")]
use std::path::Path;

#[cfg(feature = "schema")]
use wasmtime::{Engine, Instance, Memory, Module, Store, TypedFunc};

#[cfg(feature = "schema")]
use crate::error::{CliError, Result};

#[cfg(feature = "schema")]
pub struct DataDriverWasm {
    store: Store<()>,
    instance: Instance,
    memory: Memory,
}

#[cfg(feature = "schema")]
impl DataDriverWasm {
    pub fn load(wasm_path: &Path) -> Result<Self> {
        let engine = Engine::default();
        let module = Module::from_file(&engine, wasm_path)?;

        let mut store = Store::new(&engine, ());
        let instance = Instance::new(&mut store, &module, &[])?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| CliError::Message("WASM export 'memory' not found".to_string()))?;

        let init = instance
            .get_typed_func::<(), ()>(&mut store, "init")
            .map_err(|_| CliError::Message("WASM export 'init' not found".to_string()))?;
        init.call(&mut store, ())?;

        Ok(Self {
            store,
            instance,
            memory,
        })
    }

    pub fn get_schema_json(&mut self) -> Result<String> {
        let out_offset: usize = 64 * 1024;
        let out_size: usize = 256 * 1024;

        self.ensure_memory_capacity((out_offset + out_size) as u64)?;

        let get_schema = self
            .instance
            .get_typed_func::<(i32, i32), i32>(&mut self.store, "get_schema")
            .map_err(|_| CliError::Message("WASM export 'get_schema' not found".to_string()))?;

        let code = get_schema.call(&mut self.store, (out_offset as i32, out_size as i32))?;
        if code != 0 {
            let detail = self
                .read_last_error()
                .unwrap_or_else(|| "unknown error".to_string());
            return Err(CliError::Message(format!(
                "get_schema failed with code {code}: {detail}"
            )));
        }

        let bytes = self.read_prefixed_bytes(out_offset)?;
        String::from_utf8(bytes)
            .map_err(|err| CliError::Message(format!("schema output is not valid UTF-8: {err}")))
    }

    pub fn encode_input(&mut self, function: &str, json: &str) -> Result<Vec<u8>> {
        let fn_name = function.as_bytes();
        let json = json.as_bytes();

        let fn_offset = 1024usize;
        let json_offset = align_up(fn_offset + fn_name.len() + 16, 8);
        let out_offset = align_up(json_offset + json.len() + 16, 8);
        let out_size = (json.len() * 2).max(4096);

        self.ensure_memory_capacity((out_offset + out_size) as u64)?;

        self.write_bytes(fn_offset, fn_name)?;
        self.write_bytes(json_offset, json)?;

        let encode_input_fn = self
            .instance
            .get_typed_func::<(i32, i32, i32, i32, i32, i32), i32>(
                &mut self.store,
                "encode_input_fn",
            )
            .map_err(|_| {
                CliError::Message("WASM export 'encode_input_fn' not found".to_string())
            })?;

        let code = encode_input_fn.call(
            &mut self.store,
            (
                fn_offset as i32,
                fn_name.len() as i32,
                json_offset as i32,
                json.len() as i32,
                out_offset as i32,
                out_size as i32,
            ),
        )?;

        if code != 0 {
            let detail = self
                .read_last_error()
                .unwrap_or_else(|| "unknown error".to_string());
            return Err(CliError::Message(format!(
                "encode_input_fn failed with code {code}: {detail}"
            )));
        }

        self.read_prefixed_bytes(out_offset)
    }

    pub fn validate_module(wasm_path: &Path) -> Result<()> {
        let engine = Engine::default();
        let _ = Module::from_file(&engine, wasm_path)?;
        Ok(())
    }

    fn read_last_error(&mut self) -> Option<String> {
        let get_last_error: TypedFunc<(i32, i32), i32> = self
            .instance
            .get_typed_func::<(i32, i32), i32>(&mut self.store, "get_last_error")
            .ok()?;

        let out_offset: usize = 16 * 1024;
        let out_size: usize = 8 * 1024;

        self.ensure_memory_capacity((out_offset + out_size) as u64)
            .ok()?;

        let code = get_last_error
            .call(&mut self.store, (out_offset as i32, out_size as i32))
            .ok()?;
        if code != 0 {
            return None;
        }

        let bytes = self.read_prefixed_bytes(out_offset).ok()?;
        String::from_utf8(bytes).ok()
    }

    fn ensure_memory_capacity(&mut self, required_bytes: u64) -> Result<()> {
        let current_pages = self.memory.size(&mut self.store);
        let required_pages = (required_bytes + 65_535) / 65_536;

        if current_pages < required_pages {
            self.memory
                .grow(&mut self.store, required_pages - current_pages)?;
        }

        Ok(())
    }

    fn write_bytes(&mut self, offset: usize, data: &[u8]) -> Result<()> {
        let mem = self.memory.data_mut(&mut self.store);
        let end = offset + data.len();
        if end > mem.len() {
            return Err(CliError::Message(format!(
                "WASM write out of bounds (offset={offset}, len={})",
                data.len()
            )));
        }

        mem[offset..end].copy_from_slice(data);
        Ok(())
    }

    fn read_prefixed_bytes(&self, offset: usize) -> Result<Vec<u8>> {
        let data = self.memory.data(&self.store);

        if offset + 4 > data.len() {
            return Err(CliError::Message(
                "WASM output buffer out of bounds".to_string(),
            ));
        }

        let len = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize;

        let start = offset + 4;
        let end = start + len;
        if end > data.len() {
            return Err(CliError::Message(
                "WASM output exceeds memory bounds".to_string(),
            ));
        }

        Ok(data[start..end].to_vec())
    }
}

#[cfg(feature = "schema")]
fn align_up(value: usize, align: usize) -> usize {
    ((value + align - 1) / align) * align
}
