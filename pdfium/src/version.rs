#[derive(Debug, Clone, Copy)]
pub enum Version {
    Unset,
    Pdf1p0,
    Pdf1p1,
    Pdf1p2,
    Pdf1p3,
    Pdf1p4,
    Pdf1p5,
    Pdf1p6,
    Pdf1p7,
    Pdf2p0,
    Unsupported(i32),
}

impl Version {
    pub(crate) fn from_i32(version: i32) -> Self {
        match version {
            10 => Version::Pdf1p0,
            11 => Version::Pdf1p1,
            12 => Version::Pdf1p2,
            13 => Version::Pdf1p3,
            14 => Version::Pdf1p4,
            15 => Version::Pdf1p5,
            16 => Version::Pdf1p6,
            17 => Version::Pdf1p7,
            20 => Version::Pdf2p0,
            x => Version::Unsupported(x),
        }
    }

    pub(crate) fn as_i32(&self) -> Option<i32> {
        match self {
            Version::Unset => None,
            Version::Pdf1p0 => Some(10),
            Version::Pdf1p1 => Some(11),
            Version::Pdf1p2 => Some(12),
            Version::Pdf1p3 => Some(13),
            Version::Pdf1p4 => Some(14),
            Version::Pdf1p5 => Some(15),
            Version::Pdf1p6 => Some(16),
            Version::Pdf1p7 => Some(17),
            Version::Pdf2p0 => Some(20),
            Version::Unsupported(x) => Some(*x),
        }
    }

    pub fn as_major_minor(&self) -> Option<(u16, u16)> {
        let version = self.as_i32()?;

        let major = version / 10;
        let minor = version % 10;

        Some((major as _, minor as _))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some((major, minor)) = self.as_major_minor() {
            write!(f, "{}.{}", major, minor)
        } else {
            write!(f, "unset")
        }
    }
}
