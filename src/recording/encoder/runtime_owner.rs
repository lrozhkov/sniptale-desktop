//! Release the encoder before its platform runtime and thread apartment.

#[derive(Debug)]
pub(super) struct EncoderRuntimeOwners<W, R, A> {
    writer: W,
    _runtime: R,
    _apartment: A,
}

impl<W, R, A> EncoderRuntimeOwners<W, R, A> {
    pub(super) const fn new(writer: W, runtime: R, apartment: A) -> Self {
        // Rust drops fields in declaration order. Both platform guards must outlive the writer.
        Self {
            writer,
            _runtime: runtime,
            _apartment: apartment,
        }
    }

    pub(super) const fn writer(&self) -> &W {
        &self.writer
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use super::EncoderRuntimeOwners;

    struct DropProbe(&'static str, Rc<RefCell<Vec<&'static str>>>);

    impl Drop for DropProbe {
        fn drop(&mut self) {
            self.1.borrow_mut().push(self.0);
        }
    }

    #[test]
    fn encoder_release_precedes_runtime_and_apartment_on_success_and_error() {
        for fail_after_creation in [false, true] {
            let dropped = Rc::new(RefCell::new(Vec::new()));
            let run = || -> Result<(), ()> {
                let owners = EncoderRuntimeOwners::new(
                    DropProbe("writer", Rc::clone(&dropped)),
                    DropProbe("runtime", Rc::clone(&dropped)),
                    DropProbe("apartment", Rc::clone(&dropped)),
                );
                assert_eq!(owners.writer().0, "writer");
                assert!(dropped.borrow().is_empty());
                if fail_after_creation {
                    return Err(());
                }
                Ok(())
            };
            assert_eq!(run(), if fail_after_creation { Err(()) } else { Ok(()) });
            assert_eq!(*dropped.borrow(), ["writer", "runtime", "apartment"]);
        }
    }
}
