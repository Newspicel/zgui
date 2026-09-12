//! What a sizing keyword resolves to once the content has been measured.

/// The two intrinsic sizes of a box's content, in device pixels.
///
/// The engine resolves `min-content`, `max-content` and `fit-content` on a `size` or a
/// `flex-basis` itself. A `min-*` or `max-*` written with one of them takes no keyword in the
/// engine's vocabulary, so the intrinsic pre-pass measures the box and this is the shape of the
/// answer it substitutes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct IntrinsicSizes {
    /// The narrowest the content can be, in device pixels.
    pub min: f32,
    /// The widest it would like to be, in device pixels.
    pub max: f32,
}

impl IntrinsicSizes {
    /// The same two sizes with `inset` taken off each, never below zero.
    ///
    /// A measurement is of the whole box, insets included, while a size that has to have padding
    /// and border added back to it is stated without them. This converts between the two.
    #[must_use]
    pub fn less(self, inset: f32) -> Self {
        Self {
            min: (self.min - inset).max(0.0),
            max: (self.max - inset).max(0.0),
        }
    }

    /// The `fit-content` size for an available space of `available`.
    ///
    /// `fit-content` is `min(max(min-content, available), max-content)`, so a container narrower
    /// than the content gets the content's minimum and a wider one gets the content's maximum.
    pub fn fit_content(self, available: Option<f32>) -> f32 {
        match available {
            Some(available) => available.clamp(self.min, self.min.max(self.max)),
            None => self.max,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::IntrinsicSizes;

    #[test]
    fn taking_the_insets_off_a_measurement_never_goes_below_zero() {
        let sizes = IntrinsicSizes {
            min: 30.0,
            max: 90.0,
        };
        assert_eq!(
            sizes.less(20.0),
            IntrinsicSizes {
                min: 10.0,
                max: 70.0
            }
        );
        assert_eq!(sizes.less(200.0), IntrinsicSizes { min: 0.0, max: 0.0 });
    }

    #[test]
    fn fit_content_sits_between_the_two_intrinsic_sizes() {
        let sizes = IntrinsicSizes {
            min: 30.0,
            max: 90.0,
        };
        assert_eq!(sizes.fit_content(Some(10.0)), 30.0);
        assert_eq!(sizes.fit_content(Some(60.0)), 60.0);
        assert_eq!(sizes.fit_content(Some(200.0)), 90.0);
        assert_eq!(sizes.fit_content(None), 90.0);
    }
}
