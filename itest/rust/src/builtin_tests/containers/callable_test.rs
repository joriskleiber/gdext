/*
 * Copyright (c) godot-rust; Bromeon and contributors.
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use godot::builtin::inner::InnerCallable;
use godot::builtin::{
    array, varray, Array, Callable, GString, NodePath, StringName, Variant, VariantArray,
};
use godot::classes::{Node2D, Object, RefCounted};
use godot::init::GdextBuild;
use godot::meta::ToGodot;
use godot::obj::{Gd, NewAlloc, NewGd};
use godot::register::{godot_api, GodotClass};
use std::hash::Hasher;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::framework::itest;

#[derive(GodotClass)]
#[class(init, base=RefCounted)]
struct CallableTestObj {
    value: i32,
}

#[godot_api]
impl CallableTestObj {
    #[func]
    fn foo(&mut self, a: i32) {
        self.value = a;
    }

    #[func]
    fn bar(&self, b: i32) -> GString {
        b.to_variant().stringify()
    }

    #[func]
    fn baz(&self, a: i32, b: GString, c: Array<NodePath>, d: Gd<RefCounted>) -> VariantArray {
        varray![a, b, c, d]
    }

    #[func]
    fn static_function(c: i32) -> GString {
        c.to_variant().stringify()
    }
}

#[itest]
fn callable_validity() {
    let obj = CallableTestObj::new_gd();

    // non-null object, valid method
    assert!(obj.callable("foo").is_valid());
    assert!(!obj.callable("foo").is_null());
    assert!(!obj.callable("foo").is_custom());
    assert!(obj.callable("foo").object().is_some());

    // non-null object, invalid method
    assert!(!obj.callable("doesn't_exist").is_valid());
    assert!(!obj.callable("doesn't_exist").is_null());
    assert!(!obj.callable("doesn't_exist").is_custom());
    assert!(obj.callable("doesn't_exist").object().is_some());

    // null object
    assert!(!Callable::invalid().is_valid());
    assert!(Callable::invalid().is_null());
    assert!(!Callable::invalid().is_custom());
    assert_eq!(Callable::invalid().object(), None);
    assert_eq!(Callable::invalid().object_id(), None);
    assert_eq!(Callable::invalid().method_name(), None);
}

#[itest]
fn callable_hash() {
    let obj = CallableTestObj::new_gd();
    assert_eq!(obj.callable("foo").hash(), obj.callable("foo").hash());
    assert_ne!(obj.callable("foo").hash(), obj.callable("bar").hash());
}

#[itest]
fn callable_object_method() {
    let object = CallableTestObj::new_gd();
    let object_id = object.instance_id();
    let callable = object.callable("foo");

    assert_eq!(callable.object(), Some(object.clone().upcast::<Object>()));
    assert_eq!(callable.object_id(), Some(object_id));
    assert_eq!(callable.method_name(), Some("foo".into()));

    // Invalidating the object still returns the old ID, however not the object.
    drop(object);
    assert_eq!(callable.object_id(), Some(object_id));
    assert_eq!(callable.object(), None);
}

#[itest]
fn callable_static() {
    let callable = Callable::from_local_static("CallableTestObj", "static_function");

    // Test current behavior in <4.4 and >=4.4. Although our API explicitly leaves it unspecified, we then notice change in implementation.
    if cfg!(since_api = "4.4") {
        assert_eq!(callable.object(), None);
        assert_eq!(callable.object_id(), None);
        assert_eq!(callable.method_name(), None);
    } else {
        assert!(callable.object().is_some());
        assert!(callable.object_id().is_some());
        assert_eq!(callable.method_name(), Some("static_function".into()));
        assert_eq!(callable.to_string(), "GDScriptNativeClass::static_function");
    }

    // Calling works consistently everywhere.
    let result = callable.callv(&varray![12345]);
    assert_eq!(result, "12345".to_variant());

    #[cfg(since_api = "4.3")]
    assert_eq!(callable.arg_len(), 0); // Consistently doesn't work :)
}

#[itest]
fn callable_callv() {
    let obj = CallableTestObj::new_gd();
    let callable = obj.callable("foo");

    assert_eq!(obj.bind().value, 0);
    callable.callv(&varray![10]);
    assert_eq!(obj.bind().value, 10);

    // Too many arguments: this call fails, its logic is not applied.
    // In the future, panic should be propagated to caller.
    callable.callv(&varray![20, 30]);
    assert_eq!(obj.bind().value, 10);

    // TODO(bromeon): this causes a Rust panic, but since call() is routed to Godot, the panic is handled at the FFI boundary.
    // Can there be a way to notify the caller about failed calls like that?
    assert_eq!(callable.callv(&varray!["string"]), Variant::nil());

    assert_eq!(Callable::invalid().callv(&varray![1, 2, 3]), Variant::nil());
}

#[cfg(since_api = "4.2")]
#[itest]
fn callable_call() {
    let obj = CallableTestObj::new_gd();
    let callable = obj.callable("foo");

    assert_eq!(obj.bind().value, 0);
    callable.call(&[10.to_variant()]);
    assert_eq!(obj.bind().value, 10);

    // Too many arguments: this call fails, its logic is not applied.
    // In the future, panic should be propagated to caller.
    callable.call(&[20.to_variant(), 30.to_variant()]);
    assert_eq!(obj.bind().value, 10);

    // TODO(bromeon): this causes a Rust panic, but since call() is routed to Godot, the panic is handled at the FFI boundary.
    // Can there be a way to notify the caller about failed calls like that?
    assert_eq!(callable.call(&["string".to_variant()]), Variant::nil());

    assert_eq!(
        Callable::invalid().call(&[1.to_variant(), 2.to_variant(), 3.to_variant()]),
        Variant::nil()
    );
}

#[itest]
fn callable_call_return() {
    let obj = CallableTestObj::new_gd();
    let callable = obj.callable("bar");

    assert_eq!(
        callable.callv(&varray![10]),
        10.to_variant().stringify().to_variant()
    );
    // Errors in Godot, but should not crash.
    assert_eq!(callable.callv(&varray!["string"]), Variant::nil());
}

#[itest]
fn callable_call_engine() {
    let obj = Node2D::new_alloc();
    let cb = Callable::from_object_method(&obj, "set_position");
    let inner: InnerCallable = cb.as_inner();

    assert!(!inner.is_null());
    assert_eq!(inner.get_object_id(), obj.instance_id().to_i64());
    assert_eq!(inner.get_method(), StringName::from("set_position"));

    // TODO once varargs is available
    // let pos = Vector2::new(5.0, 7.0);
    // inner.call(&[pos.to_variant()]);
    // assert_eq!(obj.get_position(), pos);
    //
    // inner.bindv(array);

    obj.free();
}

#[itest]
fn callable_bindv() {
    let obj = CallableTestObj::new_gd();
    let callable = obj.callable("bar");
    let callable_bound = callable.bindv(&varray![10]);

    assert_eq!(
        callable_bound.callv(&varray![]),
        10.to_variant().stringify().to_variant()
    );
}

#[cfg(since_api = "4.2")]
#[itest]
fn callable_bind() {
    let obj = CallableTestObj::new_gd();
    let callable = obj.callable("bar");
    let callable_bound = callable.bind(&[10.to_variant()]);

    assert_eq!(
        callable_bound.call(&[]),
        10.to_variant().stringify().to_variant()
    );
}

#[cfg(since_api = "4.2")]
#[itest]
fn callable_unbind() {
    let obj = CallableTestObj::new_gd();
    let callable = obj.callable("bar");
    let callable_unbound = callable.unbind(3);

    assert_eq!(
        callable_unbound.call(&[
            121.to_variant(),
            20.to_variant(),
            30.to_variant(),
            40.to_variant()
        ]),
        121.to_variant().stringify().to_variant()
    );
}

#[cfg(since_api = "4.3")]
#[itest]
fn callable_arg_len() {
    let obj = CallableTestObj::new_gd();

    assert_eq!(obj.callable("foo").arg_len(), 1);
    assert_eq!(obj.callable("bar").arg_len(), 1);
    assert_eq!(obj.callable("baz").arg_len(), 4);
    assert_eq!(obj.callable("foo").unbind(10).arg_len(), 11);
    assert_eq!(
        obj.callable("baz")
            .bind(&[10.to_variant(), "hello".to_variant()])
            .arg_len(),
        2
    );
}

#[itest]
fn callable_bound_args_len() {
    let obj = CallableTestObj::new_gd();

    assert_eq!(obj.callable("foo").bound_args_len(), 0);
    assert_eq!(obj.callable("foo").bindv(&varray![10]).bound_args_len(), 1);
    #[cfg(since_api = "4.3")]
    assert_eq!(
        obj.callable("foo").unbind(28).bound_args_len(),
        if GdextBuild::since_api("4.4") { 0 } else { -28 }
    );
    #[cfg(since_api = "4.3")]
    assert_eq!(
        obj.callable("foo")
            .bindv(&varray![10])
            .unbind(5)
            .bound_args_len(),
        if GdextBuild::since_api("4.4") { 1 } else { -4 }
    );
}

#[itest]
fn callable_get_bound_arguments() {
    let obj = CallableTestObj::new_gd();

    let a: i32 = 10;
    let b: &str = "hello!";
    let c: Array<NodePath> = array!["my/node/path"];
    let d: Gd<RefCounted> = RefCounted::new_gd();

    let callable = obj.callable("baz");
    let callable_bound = callable.bindv(&varray![a, b, c, d]);

    assert_eq!(callable_bound.get_bound_arguments(), varray![a, b, c, d]);
}

// TODO: Add tests for `Callable::rpc` and `Callable::rpc_id`.

// Testing https://github.com/godot-rust/gdext/issues/410

#[derive(GodotClass)]
#[class(init, base = Node)]
pub struct CallableRefcountTest {}

#[godot_api]
impl CallableRefcountTest {
    #[func]
    fn accept_callable(&self, _call: Callable) {}
}

// ----------------------------------------------------------------------------------------------------------------------------------------------
// Tests and infrastructure for custom callables

#[cfg(since_api = "4.2")]
pub mod custom_callable {
    use super::*;
    use crate::framework::{assert_eq_self, quick_thread, ThreadCrosser};
    use godot::builtin::{Dictionary, RustCallable};
    use godot::sys;
    use godot::sys::GdextBuild;
    use std::fmt;
    use std::hash::Hash;
    use std::sync::{Arc, Mutex};

    #[itest]
    fn callable_from_local_fn() {
        let callable = Callable::from_local_fn("sum", sum);

        assert!(callable.is_valid());
        assert!(!callable.is_null());
        assert!(callable.is_custom());
        assert!(callable.object().is_none());

        let sum1 = callable.callv(&varray![1, 2, 4, 8]);
        assert_eq!(sum1, 15.to_variant());

        // Important to test 0 arguments, as the FFI call passes a null pointer for the argument array.
        let sum2 = callable.callv(&varray![]);
        assert_eq!(sum2, 0.to_variant());
    }

    // Without this feature, any access to the global binding from another thread fails; so the from_local_fn() cannot be tested in isolation.
    #[itest]
    fn callable_from_local_fn_crossthread() {
        // This static is a workaround for not being able to propagate failed `Callable` invocations as panics.
        // See note in itest callable_call() for further info.
        static GLOBAL: sys::Global<i32> = sys::Global::default();

        let callable = Callable::from_local_fn("change_global", |_args| {
            *GLOBAL.lock() = 777;
            Ok(Variant::nil())
        });

        // Note that Callable itself isn't Sync/Send, so we have to transfer it unsafely.
        // Godot may pass it to another thread though without `unsafe`.
        let crosser = ThreadCrosser::new(callable);

        // Create separate thread and ensure calling fails.
        // Why expect_panic for (single-threaded && Debug) but not (multi-threaded || Release) mode:
        // - Check is only enabled in Debug, not Release.
        // - We currently can't catch panics from Callable invocations, see above. True for both single/multi-threaded.
        // - In single-threaded mode, there's an FFI access check which panics as soon as another thread is invoked. *This* panics.
        // - In multi-threaded, we need to observe the effect instead (see below).

        if !cfg!(feature = "experimental-threads") && cfg!(debug_assertions) {
            // Single-threaded and Debug.
            crate::framework::expect_panic(
                "Callable created with from_local_fn() must panic when invoked on other thread",
                || {
                    quick_thread(|| {
                        let callable = unsafe { crosser.extract() };
                        callable.callv(&varray![5]);
                    });
                },
            );
        } else {
            // Multi-threaded OR Release.
            quick_thread(|| {
                let callable = unsafe { crosser.extract() };
                callable.callv(&varray![5]);
            });
        }

        assert_eq!(
            *GLOBAL.lock(),
            0,
            "Callable created with from_local_fn() must not run when invoked on other thread"
        );
    }

    #[itest]
    #[cfg(feature = "experimental-threads")]
    fn callable_from_sync_fn() {
        let callable = Callable::from_sync_fn("sum", sum);

        assert!(callable.is_valid());
        assert!(!callable.is_null());
        assert!(callable.is_custom());
        assert!(callable.object().is_none());

        let sum1 = callable.callv(&varray![1, 2, 4, 8]);
        assert_eq!(sum1, 15.to_variant());

        let sum2 = callable.callv(&varray![5]);
        assert_eq!(sum2, 5.to_variant());

        // Important to test 0 arguments, as the FFI call passes a null pointer for the argument array.
        let sum3 = callable.callv(&varray![]);
        assert_eq!(sum3, 0.to_variant());
    }

    #[itest]
    fn callable_custom_with_err() {
        let callable_with_err =
            Callable::from_local_fn("on_error_doesnt_crash", |_args: &[&Variant]| Err(()));
        // Errors in Godot, but should not crash.
        assert_eq!(callable_with_err.callv(&varray![]), Variant::nil());
    }

    #[itest]
    fn callable_from_fn_eq() {
        let a = Callable::from_local_fn("sum", sum);
        let b = a.clone();
        let c = Callable::from_local_fn("sum", sum);

        assert_eq!(a, b, "same function, same instance -> equal");
        assert_ne!(a, c, "same function, different instance -> not equal");
    }

    fn sum(args: &[&Variant]) -> Result<Variant, ()> {
        let sum: i32 = args.iter().map(|arg| arg.to::<i32>()).sum();
        Ok(sum.to_variant())
    }

    #[itest]
    fn callable_custom_invoke() {
        let my_rust_callable = Adder::new(0);
        let callable = Callable::from_custom(my_rust_callable);

        assert!(callable.is_valid());
        assert!(!callable.is_null());
        assert!(callable.is_custom());
        assert!(callable.object().is_none());

        let sum1 = callable.callv(&varray![3, 9, 2, 1]);
        assert_eq!(sum1, 15.to_variant());

        let sum2 = callable.callv(&varray![4]);
        assert_eq!(sum2, 19.to_variant());
    }

    #[itest]
    fn callable_custom_to_string() {
        let my_rust_callable = Adder::new(-2);
        let callable = Callable::from_custom(my_rust_callable);

        let variant = callable.to_variant();
        assert_eq!(variant.stringify(), GString::from("Adder(sum=-2)"));
    }

    #[itest]
    fn callable_custom_eq() {
        // Godot only invokes custom equality function if the operands are not the same instance of the Callable.

        let at = Tracker::new();
        let bt = Tracker::new();
        let ct = Tracker::new();

        let a = Callable::from_custom(Adder::new_tracked(3, at.clone()));
        let b = Callable::from_custom(Adder::new_tracked(3, bt.clone()));
        let c = Callable::from_custom(Adder::new_tracked(4, ct.clone()));

        assert_eq_self!(a);
        assert_eq!(
            eq_count(&at),
            0,
            "if it's the same Callable, Godot does not invoke custom eq"
        );

        assert_eq!(a, b);
        assert_eq!(eq_count(&at), 1);
        assert_eq!(eq_count(&bt), 1);

        assert_ne!(a, c);
        assert_eq!(eq_count(&at), 2);
        assert_eq!(eq_count(&ct), 1);

        assert_eq!(a.to_variant(), b.to_variant(), "equality inside Variant");
        assert_eq!(eq_count(&at), 3);
        assert_eq!(eq_count(&bt), 2);

        assert_ne!(a.to_variant(), c.to_variant(), "inequality inside Variant");
        assert_eq!(eq_count(&at), 4);
        assert_eq!(eq_count(&ct), 2);
    }

    #[itest]
    fn callable_custom_eq_hash() {
        // Godot only invokes custom equality function if the operands are not the same instance of the Callable.

        let at = Tracker::new();
        let bt = Tracker::new();

        let a = Callable::from_custom(Adder::new_tracked(3, at.clone()));
        let b = Callable::from_custom(Adder::new_tracked(3, bt.clone()));

        let mut dict = Dictionary::new();

        dict.set(a, "hello");
        assert_eq!(hash_count(&at), 1, "hash needed for a dict key");
        assert_eq!(eq_count(&at), 0, "eq not needed if dict bucket is empty");

        dict.set(b, "hi");
        assert_eq!(hash_count(&at), 1, "hash for a untouched if b is inserted");
        assert_eq!(hash_count(&bt), 1, "hash needed for b dict key");

        // Introduced in https://github.com/godotengine/godot/pull/96797.
        let eq = if GdextBuild::since_api("4.4") { 2 } else { 1 };

        assert_eq!(eq_count(&at), eq, "hash collision, eq for a needed");
        assert_eq!(eq_count(&bt), eq, "hash collision, eq for b needed");
    }

    #[itest]
    fn callable_callv_panic_from_fn() {
        let received = Arc::new(AtomicU32::new(0));
        let received_callable = received.clone();
        let callable = Callable::from_local_fn("test", move |_args| {
            panic!("TEST: {}", received_callable.fetch_add(1, Ordering::SeqCst))
        });

        assert_eq!(Variant::nil(), callable.callv(&varray![]));

        assert_eq!(1, received.load(Ordering::SeqCst));
    }

    #[itest]
    fn callable_callv_panic_from_custom() {
        let received = Arc::new(AtomicU32::new(0));
        let callable = Callable::from_custom(PanicCallable(received.clone()));

        assert_eq!(Variant::nil(), callable.callv(&varray![]));

        assert_eq!(1, received.load(Ordering::SeqCst));
    }

    struct Adder {
        sum: i32,

        // Track usage of PartialEq and Hash
        tracker: Arc<Mutex<Tracker>>,
    }

    impl Adder {
        fn new(sum: i32) -> Self {
            Self {
                sum,
                tracker: Tracker::new(),
            }
        }

        fn new_tracked(sum: i32, tracker: Arc<Mutex<Tracker>>) -> Self {
            Self { sum, tracker }
        }
    }

    impl PartialEq for Adder {
        fn eq(&self, other: &Self) -> bool {
            let mut guard = self.tracker.lock().unwrap();
            guard.eq_counter += 1;

            let mut guard = other.tracker.lock().unwrap();
            guard.eq_counter += 1;

            self.sum == other.sum
        }
    }

    impl Hash for Adder {
        fn hash<H: Hasher>(&self, state: &mut H) {
            let mut guard = self.tracker.lock().unwrap();
            guard.hash_counter += 1;

            self.sum.hash(state);
        }
    }

    impl fmt::Display for Adder {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "Adder(sum={})", self.sum)
        }
    }

    impl RustCallable for Adder {
        fn invoke(&mut self, args: &[&Variant]) -> Result<Variant, ()> {
            for arg in args {
                self.sum += arg.to::<i32>();
            }

            Ok(self.sum.to_variant())
        }
    }

    struct Tracker {
        eq_counter: usize,
        hash_counter: usize,
    }

    impl Tracker {
        fn new() -> Arc<Mutex<Self>> {
            Arc::new(Mutex::new(Self {
                eq_counter: 0,
                hash_counter: 0,
            }))
        }
    }

    fn eq_count(tracker: &Arc<Mutex<Tracker>>) -> usize {
        tracker.lock().unwrap().eq_counter
    }

    fn hash_count(tracker: &Arc<Mutex<Tracker>>) -> usize {
        tracker.lock().unwrap().hash_counter
    }

    // Also used in signal_test.
    pub struct PanicCallable(pub Arc<AtomicU32>);

    impl PartialEq for PanicCallable {
        fn eq(&self, other: &Self) -> bool {
            Arc::ptr_eq(&self.0, &other.0)
        }
    }

    impl Hash for PanicCallable {
        fn hash<H: Hasher>(&self, state: &mut H) {
            state.write_usize(Arc::as_ptr(&self.0) as usize)
        }
    }

    impl fmt::Display for PanicCallable {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "test")
        }
    }

    impl RustCallable for PanicCallable {
        fn invoke(&mut self, _args: &[&Variant]) -> Result<Variant, ()> {
            panic!("TEST: {}", self.0.fetch_add(1, Ordering::SeqCst))
        }
    }
}

// ----------------------------------------------------------------------------------------------------------------------------------------------
