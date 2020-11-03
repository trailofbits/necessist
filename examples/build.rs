fn main() {
    println!("cargo:rerun-if-env-changed=NECESSIST_SOURCE_FILE");
    println!("cargo:rerun-if-env-changed=NECESSIST_START_LINE");
    println!("cargo:rerun-if-env-changed=NECESSIST_START_COLUMN");
    println!("cargo:rerun-if-env-changed=NECESSIST_END_LINE");
    println!("cargo:rerun-if-env-changed=NECESSIST_END_COLUMN");
}
