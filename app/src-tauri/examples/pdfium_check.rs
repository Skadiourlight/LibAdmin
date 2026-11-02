use pdfium_render::prelude::*;

fn main() {
    let dll = libadmin_lib::config::pdfium_dll();
    println!("dll {} exists={}", dll.display(), dll.exists());
    match Pdfium::bind_to_library(&dll) {
        Ok(b) => {
            let p = Pdfium::new(b);
            if let Some(f) = std::env::args().nth(1) {
                match p.load_pdf_from_file(&f, None) {
                    Ok(doc) => println!("pages {}", doc.pages().len()),
                    Err(e) => println!("load error: {e:?}"),
                }
            }
        }
        Err(e) => println!("bind error: {e:?}"),
    }
}
