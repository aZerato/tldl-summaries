//! `tldl-manifest` : vérifie / injecte le manifest « Common-Controls v6 »
//! (ressource PE `RT_MANIFEST`, type 24 / ID 1) d'un binaire Windows.
//!
//! Pourquoi : sur la cible GNU (MinGW), le manifest par défaut du CRT *n'*
//! déclare pas `Microsoft.Windows.Common-Controls` 6.0.0.0. Sans ce manifest,
//! Windows charge `comctl32` v5, qui ne contient pas `TaskDialogIndirect`
//! (importé par la crate de dialogue `rfd`) → le processus meurt au chargement
//! avec `STATUS_ENTRYPOINT_NOT_FOUND` (0xC0000139). Le linker GNU d'ici ne
//! sait pas fusionner un manifest maison avec celui du CRT (merge `.rsrc`
//! cassé, et pas de `--no-default-manifest`) : on corrige donc le PE *après*
//! le lien, en remplaçant la ressource `RT_MANIFEST` par un manifest v6.
//!
//! Usage :
//!   tldl-manifest verify <exe>
//!   tldl-manifest patch  <exe>
//!
//! Outil autonome (seule `std`), sans dépendance réseau.

use std::env;
use std::fs;
use std::process::exit;

const RT_MANIFEST: u16 = 24;
const MANIFEST_ID: u16 = 1;

/// Manifest minimal qui active `comctl32` v6 (`TaskDialogIndirect`).
const V6_MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="TL.Summaries" version="0.1.0.0" processorArchitecture="amd64"/>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="amd64" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
</assembly>"#;

fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn w32le(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

struct Section {
    name: String,
    va: u32,
    vsize: u32,
    raw: u32,
    raw_size: u32,
}

/// (pe_sig, rsrc_rva, rsrc_size, sections)
fn parse(b: &[u8]) -> Result<(usize, u32, u32, Vec<Section>), String> {
    if b.len() < 0x40 || &b[0..2] != b"MZ" {
        return Err("pas un PE (en-tête MZ manquant)".into());
    }
    let pe = u32le(b, 0x3C) as usize;
    if b.len() < pe + 24 || &b[pe..pe + 2] != b"PE" {
        return Err("signature PE introuvable".into());
    }
    let num_sections = u16le(b, pe + 6) as usize;
    // En-tête optionnel : commence à pe+24 (magic @ opt).
    let opt = pe + 24;
    let magic = u16le(b, opt);
    // Taille de l'en-tête optionnel : 0xF0 (PE32+, magic 0x20b) / 0xE0 (PE32, 0x10b).
    let opt_size: usize = if magic == 0x20b { 0xF0 } else { 0xE0 };
    // DataDirectory : table de 16 entrées (8 o chacune). La Ressources est
    // l'index 2. La table démarre à opt+0x70 (PE32+) / opt+0x58 (PE32).
    let dd_base: usize = if magic == 0x20b { 0x70 } else { 0x58 };
    let rsrc_rva = u32le(b, opt + dd_base + 16);
    let rsrc_size = u32le(b, opt + dd_base + 20);
    let sections_start = opt + opt_size;
    let mut sections = Vec::new();
    for i in 0..num_sections {
        let s = sections_start + i * 40;
        let name = String::from_utf8_lossy(&b[s..s + 8])
            .trim_matches('\0')
            .to_string();
        // En-tête de section : Name(8), VirtualSize(4) @+8, VirtualAddress(4)
        // @+0xC, SizeOfRawData(4) @+0x10, PointerToRawData(4) @+0x14.
        let vsize = u32le(b, s + 8);
        let va = u32le(b, s + 12);
        let raw_size = u32le(b, s + 16);
        let raw = u32le(b, s + 20);
        sections.push(Section { name, va, vsize, raw, raw_size });
    }
    Ok((pe, rsrc_rva, rsrc_size, sections))
}

/// Position fichier d'un RVA (première section dont [va, va+vsize) le couvre).
fn rva_to_file(b: &[u8], sections: &[Section], rva: u32) -> Option<usize> {
    for s in sections {
        let end = s.va.wrapping_add(s.vsize.max(s.raw_size));
        if rva >= s.va && rva < end {
            let pos = (s.raw as usize) + (rva.wrapping_sub(s.va) as usize);
            return (pos < b.len()).then_some(pos);
        }
    }
    None
}

/// Trouve l'entrée de données `RT_MANIFEST`/ID 1.
/// Retourne (offset_to_data, size, pos_fichier_de_l_entrée_de_données).
fn find_manifest_data(
    b: &[u8],
    sections: &[Section],
    rsrc_rva: u32,
) -> Option<(u32, u32, usize)> {
    // Pas de ressource (DataDirectory vide) ou arborescence hors du fichier.
    if rsrc_rva == 0 {
        return None;
    }
    let base = rva_to_file(b, sections, rsrc_rva)?;
    if base + 16 > b.len() {
        return None;
    }
    // Racine : NumberOfNamedEntries @ +12, NumberOfIdEntries @ +14 ; entrées @ +16.
    let (num_named, num_id) = (u16le(b, base + 12), u16le(b, base + 14));
    if num_named > 0x100 || num_id > 0x100 {
        return None;
    }
    let mut sub24: Option<usize> = None;
    for i in 0..num_id {
        let e = base + 16 + 8 * (num_named as usize + i as usize);
        if e + 8 > b.len() {
            return None;
        }
        if (u32le(b, e) & 0xFFFF) == RT_MANIFEST as u32 {
            sub24 = Some(base + u32le(b, e + 4) as usize);
            break;
        }
    }
    let sub24 = sub24?;
    if sub24 + 16 > b.len() {
        return None;
    }
    // Sous-dossier type 24 : cherche l'ID 1 (feuille → entrée de données 16 o).
    let (nn24, nid24) = (u16le(b, sub24 + 12), u16le(b, sub24 + 14));
    if nn24 > 0x100 || nid24 > 0x100 {
        return None;
    }
    for i in 0..nid24 {
        let e = sub24 + 16 + 8 * (nn24 as usize + i as usize);
        if e + 8 > b.len() {
            return None;
        }
        if (u32le(b, e) & 0xFFFF) == MANIFEST_ID as u32 {
            let data_entry = base + u32le(b, e + 4) as usize;
            if data_entry + 8 > b.len() {
                return None;
            }
            let offset_to_data = u32le(b, data_entry);
            let size = u32le(b, data_entry + 4);
            return Some((offset_to_data, size, data_entry));
        }
    }
    None
}

fn read_manifest_bytes(b: &[u8], sections: &[Section], rsrc_rva: u32) -> Option<String> {
    let (off, size, _) = find_manifest_data(b, sections, rsrc_rva)?;
    let start = rva_to_file(b, sections, rsrc_rva.wrapping_add(off))?;
    let end = (start + size as usize).min(b.len());
    let raw = &b[start..end];
    // Le manifest est généralement UTF-8 (ou UTF-16) ; on cherche les jetons
    // caractéristiques dans les deux encodages.
    let ascii = String::from_utf8_lossy(raw);
    if !ascii.is_empty() && ascii.chars().all(|c| (c as u32) >= 0x20 || c == '\n' || c == '\r' || c == '\t') {
        return Some(ascii.into_owned());
    }
    // Fallback : UTF-16LE.
    let mut wide = String::new();
    for i in (0..raw.len() / 2 * 2).step_by(2) {
        wide.push(char::from_u32(u16le(raw, i) as u32).unwrap_or('\0'));
    }
    Some(wide)
}

fn verify(data: &[u8]) {
    let (pe, rsrc_rva, _rsrc_size, sections) = match parse(data) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ERREUR : {}", e);
            exit(4);
        }
    };
    let _ = pe;
    let manifest = match read_manifest_bytes(data, &sections, rsrc_rva) {
        Some(m) => m,
        None => {
            println!("MANIFEST_ABSENT");
            return;
        }
    };
    let has_v6 = manifest.contains("Microsoft.Windows.Common-Controls")
        && manifest.contains("6.0.0.0");
    if has_v6 {
        println!("MANIFEST_V6_OK ({} octets de manifest détectés)", manifest.len());
    } else {
        println!("MANIFEST_SANS_V6 ({} octets, pas de Common-Controls 6.0.0.0)", manifest.len());
    }
}

fn patch(data: &[u8], path: &str) {
    let (pe, rsrc_rva, _rsrc_size, sections) = match parse(data) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ERREUR : {}", e);
            exit(4);
        }
    };
    let _ = pe;
    let (off, _size, data_entry) = match find_manifest_data(data, &sections, rsrc_rva) {
        Some(t) => t,
        None => {
            eprintln!("ERREUR : pas de ressource RT_MANIFEST (24/1) à remplacer dans ce PE.");
            exit(5);
        }
    };
    let _ = off;

    // Section qui porte l'arborescence de ressources (contient rsrc_rva).
    let rsrc_sec = sections
        .iter()
        .find(|s| rsrc_rva >= s.va && rsrc_rva < s.va.wrapping_add(s.vsize.max(s.raw_size)))
        .unwrap();
    let rsrc_raw = rsrc_sec.raw as usize;
    let rsrc_raw_size = rsrc_sec.raw_size as usize;
    let align = 4usize; // alignement fichier prudent (le plus bas commun)

    let my: &[u8] = V6_MANIFEST.as_bytes();
    let my_len = my.len();

    // Position d'insertion = fin de la donnée brute de la section ressources.
    let p = rsrc_raw + rsrc_raw_size;
    // RVA de la nouvelle donnée = fin de la section (avant l'insertion).
    let new_offset_to_data = (p - rsrc_raw) as u32; // == ancienne raw_size (rsrc_rva == s.va)

    let mut buf = data.to_vec();
    // 1) Insère le manifest à la fin de la section `.rsrc`.
    buf.splice(p..p, my.iter().copied());
    // 2) Redirige l'entrée de données vers la nouvelle donnée.
    w32le(&mut buf, data_entry, new_offset_to_data);
    w32le(&mut buf, data_entry + 4, my_len as u32);
    // 3) Section `.rsrc` : grandit de my_len (arrondi à l'alignement fichier).
    let grow = (my_len + align - 1) / align * align;
    let sections_start = pe + 24 + u32le(data, pe + 8) as usize;
    // (On réutilise la position de la section via son index : on la retrouve.)
    let idx = sections
        .iter()
        .position(|s| s.raw as usize == rsrc_raw)
        .expect("section ressources");
    let s_off = sections_start + idx * 40;
    w32le(&mut buf, s_off + 16, (rsrc_raw_size + grow) as u32); // SizeOfRawData
    w32le(&mut buf, s_off + 12, (rsrc_sec.vsize + my_len as u32).max(rsrc_sec.vsize) as u32);
    // 4) Toutes les sections dont la donnée brute démarre à >= p dérivent de grow.
    for i in 0..sections.len() {
        let o = sections_start + i * 40;
        if sections[i].raw as usize >= p {
            let old = sections[i].raw as usize;
            w32le(&mut buf, o + 20, (old + grow) as u32);
        }
    }
    // 5) Écriture.
    if let Err(e) = fs::write(path, buf) {
        eprintln!("ERREUR : écriture de {} impossible : {}", path, e);
        exit(6);
    }
    println!(
        "PATCH_OK : {} octets de manifest insérés à la fin de la section (offset 0x{:x}), entrée de données 0x{:x} mise à jour.",
        my_len, new_offset_to_data, data_entry
    );
}

/// Dump de diagnostic : en-têtes + arborescence des ressources.
fn dump(data: &[u8]) {
    let (pe, rsrc_rva, rsrc_size, sections) = match parse(data) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ERREUR : {}", e);
            exit(4);
        }
    };
    println!(
        "pe={:x} opt={:x} rsrc_rva={:x} rsrc_size={:x} nsec={}",
        pe,
        pe + 24,
        rsrc_rva,
        rsrc_size,
        sections.len()
    );
    for (i, s) in sections.iter().enumerate() {
        println!(
            "  [{}] {:8} va={:x} vsize={:x} raw={:x} rawsize={:x}",
            i, s.name, s.va, s.vsize, s.raw, s.raw_size
        );
    }
    if rsrc_rva == 0 {
        println!("AUCUNE ressource (DataDirectory vide)");
        return;
    }
    let base = match rva_to_file(data, &sections, rsrc_rva) {
        Some(b) => b,
        None => {
            eprintln!("rsrc_rva non résolue");
            return;
        }
    };
    println!("root file-off={:x}", base);
    let (nn, ni) = (u16le(data, base + 12), u16le(data, base + 14));
    println!("root: named={} id={}", nn, ni);
    for i in 0..ni {
        let e = base + 16 + 8 * (nn as usize + i as usize);
        println!(
            "  root-id[{}]: id={} off=0x{:x}",
            i,
            u32le(data, e) & 0xFFFF,
            u32le(data, e + 4)
        );
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("usage : tldl-manifest <verify|patch|dump> <exe>");
        exit(2);
    }
    let cmd = args[1].clone();
    let path = args[2].clone();
    let data = match fs::read(&path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("lecture de {} impossible : {}", path, e);
            exit(3);
        }
    };
    match cmd.as_str() {
        "verify" => verify(&data),
        "patch" => patch(&data, &path),
        "dump" => dump(&data),
        other => {
            eprintln!("commande inconnue : {} (attendu : verify | patch | dump)", other);
            exit(2);
        }
    }
}
