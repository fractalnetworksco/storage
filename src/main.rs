use rocket::fs::TempFile;

#[post("/upload", format = "plain", data = "<file>")]
async fn upload(mut file: TempFile<'_>) -> std::io::Result<()> {
    file.persist_to(permanent_location).await
}

fn main() {
    println!("Hello, world!");
}
