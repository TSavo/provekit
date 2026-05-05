fn f(x: Result<u32, ()>) -> Result<u32, ()> {
    let v = x?;
    Ok(v)
}
