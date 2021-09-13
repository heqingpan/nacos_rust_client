use nacos_rust_client::client::utils::Utils;

fn main() {
    let a=b"abc-gz".to_vec();
    let b=Utils::gz_encode(&a,1);
    let c=Utils::gz_decode(&b).unwrap();
    assert_eq!(a,c);
    println!("'{}'",&String::from_utf8(c).unwrap());
}