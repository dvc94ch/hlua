use hlua_derive::lua_module;

#[lua_module]
pub mod mylib {
    pub static PI: f32 = 3.141592;

    fn function1(a: u32, b: u32) -> u32 {
        a + b
    }

    fn function2(a: u32) -> u32 {
        a + 5
    }

    #[init]
    fn init() {
        println!("mylib is now loaded!")
    }
}

fn main() {
    let mut lua = hlua::Lua::new();
    mylib::luaopen(&mut lua);
    let res: u32 = lua.execute("return mylib.function1(3, 4)").unwrap();
    assert_eq!(res, 7);
    let res: f32 = lua.execute("return mylib.PI").unwrap();
    assert_eq!(res, mylib::PI);
}
