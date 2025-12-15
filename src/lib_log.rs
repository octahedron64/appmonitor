use std::{fs::File, io::{self, Error, ErrorKind, Read, Result, Write}};

use aes::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit, block_padding::Pkcs7, generic_array::GenericArray};
use base64::{Engine, engine::general_purpose};
use rand::RngCore;
use time::{PrimitiveDateTime, UtcDateTime, UtcOffset, format_description};

const KEY: [u8; 32] = [95u8, 218, 162, 114, 88, 86, 76, 180, 196, 19, 139, 78, 122, 71, 241, 238, 176, 211, 45, 67, 189, 37, 56, 186, 236, 133, 167, 167, 133, 24, 235, 13];

#[derive(Debug, Default)]
pub struct Log {
    fpath_dat1: String,
    fpath_dat2: String,
    iv: [u8; 16],
}

pub fn log_get_filename() -> (String, String) {
    const FPATH_DAT1: &str = "\\applog.dat1";
    const FPATH_DAT2: &str = "\\applog.dat2";
    let basepath = std::env::current_exe().unwrap().to_string_lossy().to_string();
    let basepath = basepath[..basepath.rfind("\\").unwrap()].to_string();
    (basepath.clone() + FPATH_DAT1, basepath + FPATH_DAT2)
}

pub fn data_load_enc(filepath: &str) -> Result<Vec<String>> {
    let mut iv = [0u8; 16];
    let mut buf = String::new();
    let mut r = std::io::BufReader::new(std::fs::File::open(filepath)?);
    r.read(&mut iv)?;
    r.read_to_string(&mut buf)?;

    let mut v = Vec::new();
    let key = GenericArray::from_slice(&KEY);
    for l in buf.trim_end().split("\t") {
        let b = general_purpose::STANDARD.decode(l)
            .or_else(|e| Err(std::io::Error::new(ErrorKind::Other, format!("base64 error:{:?}", e))))?;
        let dec = cbc::Decryptor::<aes::Aes256Dec>::new(key, iv.as_slice().into()).decrypt_padded_vec_mut::<Pkcs7>(&b)
            .or_else(|e| Err(std::io::Error::new(ErrorKind::Other, format!("decrypt error:{:?}", e))))?;
        v.push(String::from_utf8_lossy(&dec).to_string());
    }
    Ok(v)
}

pub fn data_store_enc(filename: &str, data: &[String]) -> Result<[u8; 16]> {
    let mut rng = rand::rng();
    let iv = [rng.next_u64().to_le_bytes(), rng.next_u64().to_le_bytes()].concat();
    let key = GenericArray::from_slice(&KEY);

    let mut w = std::io::BufWriter::new(std::fs::File::create(filename)?);
    w.write_all(&iv)?;
    for l in data {
        let enc = cbc::Encryptor::<aes::Aes256Enc>::new(key, iv.as_slice().into()).encrypt_padded_vec_mut::<Pkcs7>(l.as_bytes());
        let b = general_purpose::STANDARD.encode(&enc);
        w.write_all(b.as_bytes())?;
        w.write_all("\t".as_bytes())?;
    }

    Ok(iv.try_into().unwrap()) // safe
}

impl Log {
    pub fn new<F>(filter: F) -> Result<Self> 
    where F: Fn(&[String]) -> usize
    {
        let (f1, f2) = log_get_filename();

        // ログのパージ
        let r = data_load_enc(&f1);
        let v = if r.is_err() {
            if r.as_ref().unwrap_err().kind().ne(&ErrorKind::NotFound) { return Err(r.unwrap_err()) } // safe
            Vec::new() // 初回起動でファイルがないとき
        } else { r.unwrap() }; // safe
        let filter_index = filter(&v);
        let iv = data_store_enc(&f1, &v[filter_index..])?;

        Ok(Self { fpath_dat1: f1, fpath_dat2: f2, iv })
    }

    pub fn log_write(&self, time: Option<UtcDateTime>, rec: &str) -> Result<()> {
        let time = time.unwrap_or_else(|| UtcDateTime::now());
        let ot = time.to_offset(UtcOffset::current_local_offset().unwrap());
        let l = format!("{}\t{}",
            ot.format(&format_description::parse("[year]-[month]-[day]T[hour]:[minute]:[second]").unwrap()).unwrap(), rec);

        let enc = cbc::Encryptor::<aes::Aes256Enc>::new(GenericArray::from_slice(&KEY), self.iv.as_slice().into()).encrypt_padded_vec_mut::<Pkcs7>(l.as_bytes());
        let b = general_purpose::STANDARD.encode(&enc);

        let mut f = File::options().append(true).create(true).open(&self.fpath_dat1)?;
        f.write_all(b.as_bytes())?;
        f.write_all("\t".as_bytes())?;
        Ok(())
    }

    pub fn log_load_all(&self) -> Result<Vec<String>> {
        data_load_enc(&self.fpath_dat1)
    }

    pub fn parse_record_time(l: &str) -> Result<UtcDateTime> {
        let [time, _] = l.split("\t").collect::<Vec<_>>().try_into().map_err(|_| io::Error::new(ErrorKind::InvalidData, "record invalid"))?;        
        let format = format_description::parse("[year]-[month]-[day]T[hour]:[minute]:[second]").unwrap();
        let t = PrimitiveDateTime::parse(time, &format).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        Ok(t.assume_offset(UtcOffset::current_local_offset().unwrap()).into())
    }

    pub fn load_tickfile(&self) -> Result<(UtcDateTime, UtcDateTime)> {
        let mut f = File::open(&self.fpath_dat2)?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        
        if let Ok(((t1, t2), _)) = bincode::serde::decode_from_slice::<(UtcDateTime, UtcDateTime), _>(&buf, bincode::config::standard()) {
            Ok((t1, t2))
        } else {
            Err(std::io::Error::new(ErrorKind::InvalidData, self.fpath_dat2.to_string()) )
        }
    }

    pub fn store_tickfile(&self, last_sd_log_time: UtcDateTime, hb_time: UtcDateTime) -> Result<()> {
        let mut f = File::create(&self.fpath_dat2)?;
        if let Ok(v) = bincode::serde::encode_to_vec((last_sd_log_time, hb_time), bincode::config::standard()) {
            f.write_all(v.as_slice())?;
        }
        Ok(())
    }
}

