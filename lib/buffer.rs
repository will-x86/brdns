use std::fmt;

pub type Error = Box<dyn std::error::Error>;
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone)]
pub struct BytePacketBuffer {
    pub buf: Vec<u8>, // Changed to Vec for DoH (larger responses)
    pub pos: usize,
}

impl BytePacketBuffer {
    pub fn new() -> Self {
        Self {
            buf: vec![0; 512],
            pos: 0,
        }
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            buf: vec![0; cap],
            pos: 0,
        }
    }

    pub fn from_bytes(data: &[u8]) -> Self {
        let mut buf = vec![0u8; data.len()];
        buf[..data.len()].copy_from_slice(data);
        Self { buf, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn len(&self) -> usize {
        self.pos
    }

    pub fn step(&mut self, steps: usize) -> Result<()> {
        self.pos += steps;
        Ok(())
    }

    pub fn seek(&mut self, pos: usize) -> Result<()> {
        self.pos = pos;
        Ok(())
    }

    pub fn read(&mut self) -> Result<u8> {
        if self.pos >= self.buf.len() {
            return Err("End of buffer".into());
        }
        let res = self.buf[self.pos];
        self.pos += 1;
        Ok(res)
    }

    pub fn get(&mut self, pos: usize) -> Result<u8> {
        if pos >= self.buf.len() {
            return Err("End of buffer".into());
        }
        Ok(self.buf[pos])
    }

    pub fn get_range(&mut self, start: usize, len: usize) -> Result<&[u8]> {
        if start + len > self.buf.len() {
            return Err("End of buffer".into());
        }
        Ok(&self.buf[start..start + len])
    }

    pub fn read_u16(&mut self) -> Result<u16> {
        let res = ((self.read()? as u16) << 8) | (self.read()? as u16);
        Ok(res)
    }

    pub fn read_u32(&mut self) -> Result<u32> {
        let res = ((self.read()? as u32) << 24)
            | ((self.read()? as u32) << 16)
            | ((self.read()? as u32) << 8)
            | ((self.read()? as u32) << 0);
        Ok(res)
    }

    pub fn read_qname(&mut self, outstr: &mut String) -> Result<()> {
        let mut pos = self.pos();
        let mut jumped = false;
        let mut delim = "";
        let max_jumps = 5;
        let mut jumps_performed = 0;

        loop {
            if jumps_performed > max_jumps {
                return Err(format!("Limit of {} jumps exceeded", max_jumps).into());
            }

            let len = self.get(pos)?;

            if (len & 0xC0) == 0xC0 {
                if !jumped {
                    self.seek(pos + 2)?;
                }
                let b2 = self.get(pos + 1)? as u16;
                let offset = (((len as u16) ^ 0xC0) << 8) | b2;
                pos = offset as usize;
                jumped = true;
                jumps_performed += 1;
                continue;
            }

            pos += 1;

            if len == 0 {
                break;
            }

            outstr.push_str(delim);
            let str_buffer = self.get_range(pos, len as usize)?;
            outstr.push_str(&String::from_utf8_lossy(str_buffer).to_lowercase());
            delim = ".";
            pos += len as usize;
        }

        if !jumped {
            self.seek(pos)?;
        }

        Ok(())
    }

    pub fn write(&mut self, val: u8) -> Result<()> {
        if self.pos >= self.buf.len() {
            self.buf.resize(self.pos + 1, 0);
        }
        self.buf[self.pos] = val;
        self.pos += 1;
        Ok(())
    }

    pub fn write_u8(&mut self, val: u8) -> Result<()> {
        self.write(val)
    }

    pub fn write_u16(&mut self, val: u16) -> Result<()> {
        self.write((val >> 8) as u8)?;
        self.write((val & 0xFF) as u8)
    }

    pub fn write_u32(&mut self, val: u32) -> Result<()> {
        self.write(((val >> 24) & 0xFF) as u8)?;
        self.write(((val >> 16) & 0xFF) as u8)?;
        self.write(((val >> 8) & 0xFF) as u8)?;
        self.write(((val >> 0) & 0xFF) as u8)
    }

    pub fn write_qname(&mut self, qname: &str) -> Result<()> {
        for label in qname.split('.') {
            let len = label.len();
            if len > 0x3f {
                return Err("Single label exceeds 63 characters".into());
            }
            self.write_u8(len as u8)?;
            for b in label.as_bytes() {
                self.write_u8(*b)?;
            }
        }
        self.write_u8(0)
    }

    /// Get the written portion of the buffer
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.pos]
    }
}

impl fmt::Debug for BytePacketBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BytePacketBuffer")
            .field("pos", &self.pos)
            .field("len", &self.buf.len())
            .finish()
    }
}
