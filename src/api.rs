use rocket::fs::TempFile;
use rocket::*;
use rocket::data::ToByteUnit;
use crate::keys::Pubkey;

#[post("/snapshot/<volume>/upload", format = "plain", data = "<data>")]
async fn upload(mut data: Data<'_>, volume: Pubkey) -> std::io::Result<()> {
    let stream = data.open(10.gibibytes());
    //file.persist_to(permanent_location).await
    Ok(())
}

pub fn routes() -> Vec<Route> {
    routes![upload]
}
