use rocket::request::FromParam;

pub struct Pubkey([u8; 32]);
pub struct Privkey([u8; 32]);

impl<'r> FromParam<'r> for Pubkey {
    type Error = &'r str;

    fn from_param(param: &'r str) -> Result<Self, Self::Error> {
        unimplemented!()
    }
}
