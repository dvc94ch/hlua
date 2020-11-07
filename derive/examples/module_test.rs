use hlua_derive::lua;

#[lua]
pub mod mylib {
    pub static PI: f32 = 3.141592;

    fn function1(a: u32, b: u32) -> u32 {
        a + b
    }

    fn function2(a: u32) -> u32 {
        a + 5
    }
}

#[lua]
pub mod vec3 {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Vec3(f64, f64, f64);

    impl Vec3 {
        pub fn new(x: f64, y: f64, z: f64) -> Self {
            Self(x, y, z)
        }

        #[lua(meta = "__add")]
        pub fn add(&self, other: &Vec3) -> Self {
            Self(self.0 + other.0, self.1 + other.1, self.2 + other.2)
        }

        #[lua(meta = "__tostring")]
        pub fn to_string(&self) -> String {
            format!("[ {}, {}, {} ]", self.0, self.1, self.2)
        }
    }
}

pub struct LuaSandbox<'lua> {
    lua: hlua::Lua<'lua>,
}

impl<'lua> LuaSandbox<'lua> {
    const USER_FUNCTION_NAME: &'static str = "__user_function__";
    const SANDBOX_ENV_NAME: &'static str = "__sandbox_env__";
    // This env is taken from:
    // http://stackoverflow.com/questions/1224708/how-can-i-create-a-secure-lua-sandbox
    const SANDBOX_ENV: &'static str = "{
        ipairs = ipairs,
        next = next,
        pairs = pairs,
        pcall = pcall,
        print = print,
        tonumber = tonumber,
        tostring = tostring,
        type = type,
        unpack = unpack,
        coroutine = { create = coroutine.create, resume = coroutine.resume,
            running = coroutine.running, status = coroutine.status,
            wrap = coroutine.wrap },
        string = { byte = string.byte, char = string.char, find = string.find,
            format = string.format, gmatch = string.gmatch, gsub = string.gsub,
            len = string.len, lower = string.lower, match = string.match,
            rep = string.rep, reverse = string.reverse, sub = string.sub,
            upper = string.upper },
        table = { insert = table.insert, maxn = table.maxn, remove = table.remove,
            sort = table.sort },
        math = { abs = math.abs, acos = math.acos, asin = math.asin,
            atan = math.atan, atan2 = math.atan2, ceil = math.ceil, cos = math.cos,
            cosh = math.cosh, deg = math.deg, exp = math.exp, floor = math.floor,
            fmod = math.fmod, frexp = math.frexp, huge = math.huge,
            ldexp = math.ldexp, log = math.log, log10 = math.log10, max = math.max,
            min = math.min, modf = math.modf, pi = math.pi, pow = math.pow,
            rad = math.rad, random = math.random, sin = math.sin, sinh = math.sinh,
            sqrt = math.sqrt, tan = math.tan, tanh = math.tanh },
        os = { clock = os.clock, difftime = os.difftime, time = os.time },
    }";

    pub fn new() -> Result<Self, hlua::LuaError> {
        let mut lua = hlua::Lua::new();
        lua.openlibs();
        lua.execute(&format!(
            "{env} = {val}",
            env = Self::SANDBOX_ENV_NAME,
            val = Self::SANDBOX_ENV,
        ))?;
        Ok(Self { lua })
    }

    pub fn load<'a, F>(&'a mut self, luaopen: &F)
    where
        F: Fn(hlua::LuaTable<hlua::PushGuard<&'a mut hlua::Lua<'lua>>>),
    {
        let env = self.lua.get(Self::SANDBOX_ENV_NAME).unwrap();
        luaopen(env);
    }

    pub fn execute<'a, T>(&'a mut self, script: &str) -> Result<T, hlua::LuaError>
    where
        T: for<'g> hlua::LuaRead<hlua::PushGuard<&'g mut hlua::PushGuard<&'a mut hlua::Lua<'lua>>>>,
    {
        self.lua
            .checked_set(Self::USER_FUNCTION_NAME, hlua::LuaCode(script))?;
        self.lua.execute(&format!(
            "debug.setupvalue({f}, 1, {env}); return {}();",
            f = Self::USER_FUNCTION_NAME,
            env = Self::SANDBOX_ENV_NAME,
        ))
    }
}

fn main() {
    let mut lua = LuaSandbox::new().unwrap();
    lua.load(&mylib::load);
    let res: u32 = lua.execute("return mylib.function1(3, 4)").unwrap();
    assert_eq!(res, 7);
    let res: f32 = lua.execute("return mylib.PI").unwrap();
    assert_eq!(res, mylib::PI);

    lua.load(&vec3::load);
    let res: vec3::Vec3 = lua
        .execute("return vec3.new(1.0, 2.0, 3.0) + vec3.new(3.0, 2.0, 1.0)")
        .unwrap();
    assert_eq!(res, vec3::Vec3::new(4.0, 4.0, 4.0));
    println!("result is {}", res.to_string());
}
