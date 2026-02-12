use std::{env, fs, ops::Range, path::PathBuf};

use anyhow::{Context, Result, bail, ensure};
use wasm_encoder::{Module, RawSection, SectionId};
use wasmparser::{
    BinaryReader, DataKind, DataSectionReader, Export, ExportSectionReader, ExternalKind,
    FunctionBody, ImportSectionReader, MemorySectionReader, Operator, Parser, Payload, TypeRef,
};

const DEFAULT_EXPORT: &str = "lasr_script";
const WASM_PAGE_SIZE: u64 = 65536;
const LASR_RUNTIME_WASM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/lasr_runtime.wasm"));

fn main() -> Result<()> {
    let args = Args::parse(env::args().skip(1).collect())?;

    let script = fs::read(&args.script_lua)
        .with_context(|| format!("failed to read {}", args.script_lua.display()))?;

    let output = inject_script(LASR_RUNTIME_WASM, &script, DEFAULT_EXPORT)?;

    fs::write(&args.output_wasm, output)
        .with_context(|| format!("failed to write {}", args.output_wasm.display()))?;

    Ok(())
}

struct Args {
    script_lua: PathBuf,
    output_wasm: PathBuf,
}

impl Args {
    fn parse(args: Vec<String>) -> Result<Self> {
        ensure!(
            !args.is_empty(),
            "usage: lasr-compiler <script.lua> [out.wasm]"
        );
        ensure!(
            args.len() <= 2,
            "usage: lasr-compiler <script.lua> [out.wasm]"
        );

        let script_lua = PathBuf::from(&args[0]);

        let output_wasm = args
            .get(1)
            .map(PathBuf::from)
            .unwrap_or_else(|| script_lua.with_extension("wasm"));

        Ok(Self {
            script_lua,
            output_wasm,
        })
    }
}

fn inject_script(wasm: &[u8], script: &[u8], export_name: &str) -> Result<Vec<u8>> {
    let mut section_order: Vec<SectionItem<'_>> = Vec::new();
    let mut code_body_ranges: Vec<Range<usize>> = Vec::new();
    let mut data_segments: Vec<(u32, i32, Vec<u8>)> = Vec::new();
    let mut func_imports = 0u32;
    let mut export_func_index: Option<u32> = None;
    let mut export_entries: Vec<ExportEntry> = Vec::new();
    let mut has_code_section = false;
    let mut has_data_section = false;
    let mut memory_limits: Option<MemoryLimits> = None;

    for payload in Parser::new(0).parse_all(wasm) {
        let payload = payload?;
        let raw_section = raw_section_from_payload(&payload, wasm)?;
        match payload {
            Payload::Version { .. } => {}
            Payload::ImportSection(reader) => {
                func_imports = count_func_imports(reader)?;
                if let Some(section) = raw_section {
                    section_order.push(SectionItem::Raw(section));
                }
            }
            Payload::ExportSection(reader) => {
                let (entries, export_index) = read_exports(reader, export_name)?;
                export_entries = entries;
                export_func_index = export_index;
                section_order.push(SectionItem::Export);
            }
            Payload::MemorySection(reader) => {
                memory_limits = Some(read_memory_section(reader)?);
                section_order.push(SectionItem::Memory);
            }
            Payload::CodeSectionStart { .. } => {
                has_code_section = true;
                section_order.push(SectionItem::Code);
            }
            Payload::CodeSectionEntry(body) => {
                code_body_ranges.push(body.range());
            }
            Payload::DataSection(reader) => {
                data_segments = read_data_segments(reader)?;
                has_data_section = true;
                section_order.push(SectionItem::Data);
            }
            Payload::End(_) => {
                if let Payload::End(_) = payload {
                    break;
                }
            }
            _ => {
                if let Some(section) = raw_section {
                    section_order.push(SectionItem::Raw(section));
                }
            }
        }
    }

    let export_func_index = export_func_index.context(format!("export {export_name} not found"))?;

    let export_code_index = export_func_index
        .checked_sub(func_imports)
        .context("export refers to imported function")?;

    let memory_limits = memory_limits.context("module has no memory")?;
    let (data_offset, script_len, new_initial_pages) =
        append_script_data(&mut data_segments, script, &memory_limits)?;

    ensure!(has_code_section, "module has no code section");

    ensure!(
        (export_code_index as usize) < code_body_ranges.len(),
        "export function index out of range"
    );

    if !has_data_section {
        section_order.push(SectionItem::Data);
    }

    let patch_index = resolve_patch_index(
        wasm,
        &code_body_ranges,
        export_code_index as usize,
        func_imports,
    )?;
    let code_section = build_code_section(
        wasm,
        &code_body_ranges,
        patch_index,
        export_code_index as usize,
        data_offset,
        script_len,
    )?;
    let data_section = build_data_section(&data_segments);
    let export_section = build_export_section(&export_entries);
    let memory_section = build_memory_section(new_initial_pages, memory_limits.maximum)?;

    let mut module = Module::new();
    for item in section_order {
        match item {
            SectionItem::Raw(section) => {
                module.section(&section);
            }
            SectionItem::Code => {
                let section = RawSection {
                    id: SectionId::Code as u8,
                    data: &code_section,
                };
                module.section(&section);
            }
            SectionItem::Export => {
                let section = RawSection {
                    id: SectionId::Export as u8,
                    data: &export_section,
                };
                module.section(&section);
            }
            SectionItem::Memory => {
                let section = RawSection {
                    id: SectionId::Memory as u8,
                    data: &memory_section,
                };
                module.section(&section);
            }
            SectionItem::Data => {
                let section = RawSection {
                    id: SectionId::Data as u8,
                    data: &data_section,
                };
                module.section(&section);
            }
        }
    }

    Ok(module.finish())
}

fn count_func_imports(reader: ImportSectionReader) -> Result<u32> {
    let mut count = 0u32;
    for import in reader.into_imports() {
        let import = import?;
        if let TypeRef::Func(_) | TypeRef::FuncExact(_) = import.ty {
            count += 1;
        }
    }
    Ok(count)
}

fn read_exports(
    reader: ExportSectionReader,
    export_name: &str,
) -> Result<(Vec<ExportEntry>, Option<u32>)> {
    let mut entries = Vec::new();
    let mut export_index = None;
    for export in reader {
        let export: Export = export?;
        if export.name == export_name
            && let ExternalKind::Func = export.kind
        {
            export_index = Some(export.index);
            continue;
        }
        entries.push(ExportEntry {
            name: export.name.to_owned(),
            kind: export.kind,
            index: export.index,
        });
    }
    Ok((entries, export_index))
}

fn resolve_patch_index(
    wasm: &[u8],
    bodies: &[Range<usize>],
    export_index: usize,
    func_imports: u32,
) -> Result<usize> {
    let range = bodies
        .get(export_index)
        .context("export function body missing")?;
    let reader = BinaryReader::new(&wasm[range.clone()], range.start);
    let body = FunctionBody::new(reader);
    let mut reader = body.get_operators_reader()?;
    while !reader.eof() {
        if let Operator::Call { function_index } = reader.read()?
            && function_index >= func_imports
        {
            let index = function_index - func_imports;
            let index = index.try_into().context("call target index overflow")?;
            if index < bodies.len() {
                return Ok(index);
            }
        }
    }
    Ok(export_index)
}

fn read_memory_section(reader: MemorySectionReader) -> Result<MemoryLimits> {
    let mut memories = reader.into_iter();
    let memory = memories.next().context("memory section is empty")??;
    ensure!(
        memories.next().is_none(),
        "multiple memories are not supported"
    );
    ensure!(
        !(memory.shared || memory.memory64),
        "shared or memory64 is not supported"
    );
    let initial = memory
        .initial
        .try_into()
        .context("memory initial too large")?;
    let maximum = match memory.maximum {
        Some(value) => Some(value.try_into().context("memory maximum too large")?),
        None => None,
    };
    Ok(MemoryLimits { initial, maximum })
}

fn read_data_segments(reader: DataSectionReader) -> Result<Vec<(u32, i32, Vec<u8>)>> {
    let mut segments = Vec::new();
    for segment in reader {
        let segment = segment?;
        match segment.kind {
            DataKind::Active {
                memory_index,
                offset_expr,
            } => {
                let offset = parse_i32_const(offset_expr)?;
                segments.push((memory_index, offset, segment.data.to_vec()));
            }
            DataKind::Passive => {
                bail!("unsupported data segment kind (only active i32.const offsets supported)");
            }
        }
    }
    Ok(segments)
}

fn parse_i32_const(expr: wasmparser::ConstExpr) -> Result<i32> {
    let mut reader = expr.get_operators_reader();
    let op = reader.read()?;
    let offset = match op {
        wasmparser::Operator::I32Const { value } => value,
        _ => bail!("unsupported data offset expression"),
    };
    let end = reader.read()?;
    ensure!(
        matches!(end, wasmparser::Operator::End),
        "malformed data offset expression"
    );
    Ok(offset)
}

fn append_script_data(
    segments: &mut Vec<(u32, i32, Vec<u8>)>,
    script: &[u8],
    memory: &MemoryLimits,
) -> Result<(i32, i32, u32)> {
    let base_offset = u64::from(memory.initial)
        .checked_mul(WASM_PAGE_SIZE)
        .context("memory size overflow")?;
    let len: i32 = script.len().try_into().context("script too large")?;
    let aligned = (base_offset + 15) & !15;
    let end_offset = aligned
        .checked_add(len as u64)
        .context("script offset overflow")?;

    let required_pages: u32 = end_offset
        .div_ceil(WASM_PAGE_SIZE)
        .try_into()
        .context("required pages overflow")?;
    let new_initial = required_pages.max(memory.initial);
    ensure!(
        memory.maximum.is_none_or(|max| new_initial <= max),
        "script does not fit within maximum memory size"
    );

    let offset_i32 = aligned.try_into().context("script offset too large")?;
    segments.push((0, offset_i32, script.to_vec()));
    Ok((offset_i32, len, new_initial))
}

fn build_code_section(
    wasm: &[u8],
    bodies: &[Range<usize>],
    replace_index: usize,
    wrapper_index: usize,
    ptr: i32,
    len: i32,
) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    push_u32_leb(bodies.len() as u32, &mut data);

    for (index, range) in bodies.iter().enumerate() {
        let body = if index == replace_index {
            build_script_body_sret(ptr, len)
        } else if index == wrapper_index && wrapper_index != replace_index {
            build_empty_body()
        } else {
            wasm[range.clone()].to_vec()
        };
        push_u32_leb(body.len() as u32, &mut data);
        data.extend_from_slice(&body);
    }

    Ok(data)
}

fn build_script_body_sret(ptr: i32, len: i32) -> Vec<u8> {
    let mut body = Vec::new();
    push_u32_leb(0, &mut body);
    body.push(0x20);
    body.push(0x00);
    body.push(0x41);
    push_i32_leb(len, &mut body);
    body.push(0x36);
    body.push(0x02);
    body.push(0x04);
    body.push(0x20);
    body.push(0x00);
    body.push(0x41);
    push_i32_leb(ptr, &mut body);
    body.push(0x36);
    body.push(0x02);
    body.push(0x00);
    body.push(0x0b);
    body
}

fn build_empty_body() -> Vec<u8> {
    let mut body = Vec::new();
    push_u32_leb(0, &mut body);
    body.push(0x0b);
    body
}

fn build_data_section(segments: &[(u32, i32, Vec<u8>)]) -> Vec<u8> {
    let mut data = Vec::new();
    push_u32_leb(segments.len() as u32, &mut data);
    for (mem, offset, bytes) in segments {
        if *mem == 0 {
            data.push(0x00);
        } else {
            data.push(0x02);
            push_u32_leb(*mem, &mut data);
        }
        data.push(0x41);
        push_i32_leb(*offset, &mut data);
        data.push(0x0b);
        push_u32_leb(bytes.len() as u32, &mut data);
        data.extend_from_slice(bytes);
    }
    data
}

fn build_export_section(entries: &[ExportEntry]) -> Vec<u8> {
    let mut data = Vec::new();
    push_u32_leb(entries.len() as u32, &mut data);
    for entry in entries {
        push_name(&entry.name, &mut data);
        data.push(export_kind_byte(entry.kind));
        push_u32_leb(entry.index, &mut data);
    }
    data
}

fn export_kind_byte(kind: ExternalKind) -> u8 {
    match kind {
        ExternalKind::Func => 0x00,
        ExternalKind::Table => 0x01,
        ExternalKind::Memory => 0x02,
        ExternalKind::Global => 0x03,
        ExternalKind::Tag => 0x04,
        ExternalKind::FuncExact => unreachable!("FuncExact is not valid in exports"),
    }
}

fn build_memory_section(initial: u32, maximum: Option<u32>) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    push_u32_leb(1, &mut data);
    match maximum {
        Some(max) => {
            data.push(0x01);
            push_u32_leb(initial, &mut data);
            push_u32_leb(max, &mut data);
        }
        None => {
            data.push(0x00);
            push_u32_leb(initial, &mut data);
        }
    }
    Ok(data)
}

fn push_u32_leb(mut value: u32, out: &mut Vec<u8>) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            break;
        }
        out.push(byte | 0x80);
    }
}

fn push_name(value: &str, out: &mut Vec<u8>) {
    push_u32_leb(value.len() as u32, out);
    out.extend_from_slice(value.as_bytes());
}

fn push_i32_leb(mut value: i32, out: &mut Vec<u8>) {
    loop {
        let byte = (value & 0x7f) as u8;
        let sign_bit = (byte & 0x40) != 0;
        value >>= 7;
        let done = (value == 0 && !sign_bit) || (value == -1 && sign_bit);
        if done {
            out.push(byte);
            break;
        }
        out.push(byte | 0x80);
    }
}

fn raw_section_from_payload<'a>(
    payload: &Payload<'a>,
    wasm: &'a [u8],
) -> Result<Option<RawSection<'a>>> {
    let Some((id, range)) = payload.as_section() else {
        return Ok(None);
    };
    Ok(Some(RawSection {
        id,
        data: &wasm[range],
    }))
}

enum SectionItem<'a> {
    Raw(RawSection<'a>),
    Code,
    Export,
    Memory,
    Data,
}

struct MemoryLimits {
    initial: u32,
    maximum: Option<u32>,
}

struct ExportEntry {
    name: String,
    kind: ExternalKind,
    index: u32,
}
