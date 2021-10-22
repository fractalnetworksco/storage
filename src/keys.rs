use rocket::http::Status;
use rocket::request::FromParam;

pub struct Pubkey([u8; 32]);
pub struct Privkey([u8; 32]);

impl<'r> FromParam<'r> for Pubkey {
    type Error = &'r str;

    fn from_param(param: &'r str) -> Result<Self, Self::Error> {
        let mut key = Pubkey([0; 32]);
        match hex::decode_to_slice(param, &mut key.0 as &mut [u8]) {
            Ok(_) => Ok(key),
            Err(e) => Err(param),
        }
    }
}

impl Pubkey {
    pub fn as_slice(&self) -> &[u8] {
        &self.0[..]
    }

    pub fn to_hex(&self) -> String {
        hex::encode(&self.0)
    }
}
