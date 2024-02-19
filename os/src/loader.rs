//! Loading user applications into memory
use alloc::vec::Vec;
use lazy_static::*;

/// Get the total number of applications.
pub fn get_num_app() -> usize {
    extern "C" {
        fn _num_app();
    }
    unsafe { (_num_app as usize as *const usize).read_volatile() }
}

// get_app_data 则根据传入的应用编号取出对应应用的 ELF 格式可执行文件数据
/// get applications data
pub fn get_app_data(app_id: usize) -> &'static [u8] {
    extern "C" {
        fn _num_app();
    }
    let num_app_ptr = _num_app as usize as *const usize;
    let num_app = get_num_app();
    let app_start = unsafe { core::slice::from_raw_parts(num_app_ptr.add(1), num_app + 1) };
    assert!(app_id < num_app);
    unsafe {
        core::slice::from_raw_parts(
            app_start[app_id] as *const u8,
            app_start[app_id + 1] - app_start[app_id],
        )
    }
}

// 分析 link_app.S 中的内容，并用一个全局可见的 只读 向量 APP_NAMES 来按照顺序将所有应用的名字保存在内存中
lazy_static! {
    ///All of app's name
    static ref APP_NAMES: Vec<&'static str> = {
        let num_app = get_num_app();
        extern "C" {
            fn _app_names();
        }
        let mut start = _app_names as usize as *const u8;
        let mut v = Vec::new();
        unsafe {
            for _ in 0..num_app {
                let mut end = start;
                while end.read_volatile() != b'\0' {
                    end = end.add(1);
                }
                let slice = core::slice::from_raw_parts(start, end as usize - start as usize);
                let str = core::str::from_utf8(slice).unwrap();
                v.push(str);
                start = end.add(1);
            }
        }
        v
    };
}

#[allow(unused)]
// 按照应用的名字来查找获得应用的 ELF 数据
///get app data from name
pub fn get_app_data_by_name(name: &str) -> Option<&'static [u8]> {
    let num_app = get_num_app();
    (0..num_app)
        .find(|&i| APP_NAMES[i] == name)
        .map(get_app_data)
}
// list_apps 在内核初始化时被调用，它可以打印出所有可用的应用的名字
///list all apps
pub fn list_apps() {
    println!("/**** APPS ****");
    for app in APP_NAMES.iter() {
        println!("{}", app);
    }
    println!("**************/");
}


// use crate::config::*;
// use crate::trap::TrapContext;
// use core::arch::asm;

// #[repr(align(4096))]
// #[derive(Copy, Clone)]
// struct KernelStack {
//     data: [u8; KERNEL_STACK_SIZE],
// }

// #[repr(align(4096))]
// #[derive(Copy, Clone)]
// struct UserStack {
//     data: [u8; USER_STACK_SIZE],
// }

// static KERNEL_STACK: [KernelStack; MAX_APP_NUM] = [KernelStack {
//     data: [0; KERNEL_STACK_SIZE],
// }; MAX_APP_NUM];

// static USER_STACK: [UserStack; MAX_APP_NUM] = [UserStack {
//     data: [0; USER_STACK_SIZE],
// }; MAX_APP_NUM];

// impl KernelStack {
//     fn get_sp(&self) -> usize {
//         self.data.as_ptr() as usize + KERNEL_STACK_SIZE
//     }
//     pub fn push_context(&self, trap_cx: TrapContext) -> usize {
//         let trap_cx_ptr = (self.get_sp() - core::mem::size_of::<TrapContext>()) as *mut TrapContext;
//         unsafe {
//             *trap_cx_ptr = trap_cx;
//         }
//         trap_cx_ptr as usize
//     }
// }

// impl UserStack {
//     fn get_sp(&self) -> usize {
//         self.data.as_ptr() as usize + USER_STACK_SIZE
//     }
// }

// /// Get base address of app i.
// fn get_base_i(app_id: usize) -> usize {
//     APP_BASE_ADDRESS + app_id * APP_SIZE_LIMIT
// }

// /// Get the total number of applications.
// pub fn get_num_app() -> usize {
//     extern "C" {
//         fn _num_app();
//     }
//     unsafe { (_num_app as usize as *const usize).read_volatile() }
// }

// /// Load nth user app at
// /// [APP_BASE_ADDRESS + n * APP_SIZE_LIMIT, APP_BASE_ADDRESS + (n+1) * APP_SIZE_LIMIT).
// pub fn load_apps() {
//     extern "C" {
//         fn _num_app();
//     }
//     let num_app_ptr = _num_app as usize as *const usize;
//     let num_app = get_num_app();
//     let app_start = unsafe { core::slice::from_raw_parts(num_app_ptr.add(1), num_app + 1) };
//     // clear i-cache first
//     unsafe {
//         asm!("fence.i");
//     }
//     // load apps
//     for i in 0..num_app {
//         // 第i个应用被加载到以物理地址 base_i 开头的一段物理内存上
//         let base_i = get_base_i(i);
//         // clear region
//         (base_i..base_i + APP_SIZE_LIMIT)
//             .for_each(|addr| unsafe { (addr as *mut u8).write_volatile(0) });
//         // load app from data section to memory
//         let src = unsafe {
//             core::slice::from_raw_parts(app_start[i] as *const u8, app_start[i + 1] - app_start[i])
//         };
//         let dst = unsafe { core::slice::from_raw_parts_mut(base_i as *mut u8, src.len()) };
//         dst.copy_from_slice(src);
//     }
// }

// /// get app info with entry and sp and save `TrapContext` in kernel stack
// pub fn init_app_cx(app_id: usize) -> usize {
//     KERNEL_STACK[app_id].push_context(TrapContext::app_init_context(
//         get_base_i(app_id),
//         USER_STACK[app_id].get_sp(),
//     ))
// }
