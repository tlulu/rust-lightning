//! Feature flag definitions for the Lightning protocol according to [BOLT #9].
//!
//! Lightning nodes advertise a supported set of operation through feature flags. Features are
//! applicable for a specific context as indicated in some [messages]. [`Features`] encapsulates
//! behavior for specifying and checking feature flags for a particular context. Each feature is
//! defined internally by a trait specifying the corresponding flags (i.e., even and odd bits). A
//! [`Context`] is used to parameterize [`Features`] and defines which features it can support.
//!
//! Whether a feature is considered "known" or "unknown" is relative to the implementation, whereas
//! the term "supports" is used in reference to a particular set of [`Features`]. That is, a node
//! supports a feature if it advertises the feature (as either required or optional) to its peers.
//! And the implementation can interpret a feature if the feature is known to it.
//!
//! [BOLT #9]: https://github.com/lightningnetwork/lightning-rfc/blob/master/09-features.md
//! [messages]: ../msgs/index.html
//! [`Features`]: struct.Features.html
//! [`Context`]: sealed/trait.Context.html

use std::{cmp, fmt};
use std::result::Result;
use std::marker::PhantomData;

use ln::msgs::DecodeError;
use util::ser::{Readable, Writeable, Writer};

mod sealed {
	/// The context in which [`Features`] are applicable. Defines which features are required and
	/// which are optional for the context.
	///
	/// [`Features`]: ../struct.Features.html
	pub trait Context {
		/// Features that are known to the implementation, where a required feature is indicated by
		/// its even bit and an optional feature is indicated by its odd bit.
		const KNOWN_FEATURE_FLAGS: &'static [u8];

		/// Bitmask for selecting features that are known to the implementation, regardless of
		/// whether each feature is required or optional.
		const KNOWN_FEATURE_MASK: &'static [u8];
	}

	/// Defines a [`Context`] by stating which features it requires and which are optional. Features
	/// are specified as a comma-separated list of bytes where each byte is a pipe-delimited list of
	/// feature identifiers.
	///
	/// [`Context`]: trait.Context.html
	macro_rules! define_context {
		($context: ident {
			required_features: [$( $( $required_feature: ident )|*, )*],
			optional_features: [$( $( $optional_feature: ident )|*, )*],
		}) => {
			pub struct $context {}

			impl Context for $context {
				const KNOWN_FEATURE_FLAGS: &'static [u8] = &[
					// For each byte, use bitwise-OR to compute the applicable flags for known
					// required features `r_i` and optional features `o_j` for all `i` and `j` such
					// that the following slice is formed:
					//
					// [
					//  `r_0` | `r_1` | ... | `o_0` | `o_1` | ...,
					//  ...,
					// ]
					$(
						0b00_00_00_00 $(|
							<Self as $required_feature>::REQUIRED_MASK)*
						$(|
							<Self as $optional_feature>::OPTIONAL_MASK)*,
					)*
				];

				const KNOWN_FEATURE_MASK: &'static [u8] = &[
					// Similar as above, but set both flags for each feature regardless of whether
					// the feature is required or optional.
					$(
						0b00_00_00_00 $(|
							<Self as $required_feature>::REQUIRED_MASK |
							<Self as $required_feature>::OPTIONAL_MASK)*
						$(|
							<Self as $optional_feature>::REQUIRED_MASK |
							<Self as $optional_feature>::OPTIONAL_MASK)*,
					)*
				];
			}
		};
	}

	define_context!(InitContext {
		required_features: [
			// Byte 0
			,
			// Byte 1
			,
			// Byte 2
			,
		],
		optional_features: [
			// Byte 0
			DataLossProtect | InitialRoutingSync | UpfrontShutdownScript,
			// Byte 1
			VariableLengthOnion | PaymentSecret,
			// Byte 2
			BasicMPP,
		],
	});
	define_context!(NodeContext {
		required_features: [
			// Byte 0
			,
			// Byte 1
			,
			// Byte 2
			,
		],
		optional_features: [
			// Byte 0
			DataLossProtect | UpfrontShutdownScript,
			// Byte 1
			VariableLengthOnion | PaymentSecret,
			// Byte 2
			BasicMPP,
		],
	});
	define_context!(ChannelContext {
		required_features: [],
		optional_features: [],
	});

	/// Defines a feature with the given bits for the specified [`Context`]s. The generated trait is
	/// useful for manipulating feature flags.
	///
	/// [`Context`]: trait.Context.html
	macro_rules! define_feature {
		($odd_bit: expr, $feature: ident, [$($context: ty),+], $doc: expr) => {
			#[doc = $doc]
			///
			/// See [BOLT #9] for details.
			///
			/// [BOLT #9]: https://github.com/lightningnetwork/lightning-rfc/blob/master/09-features.md
			pub trait $feature: Context {
				/// The bit used to signify that the feature is required.
				const EVEN_BIT: usize = $odd_bit - 1;

				/// The bit used to signify that the feature is optional.
				const ODD_BIT: usize = $odd_bit;

				/// Assertion that [`EVEN_BIT`] is actually even.
				///
				/// [`EVEN_BIT`]: #associatedconstant.EVEN_BIT
				const ASSERT_EVEN_BIT_PARITY: usize;

				/// Assertion that [`ODD_BIT`] is actually odd.
				///
				/// [`ODD_BIT`]: #associatedconstant.ODD_BIT
				const ASSERT_ODD_BIT_PARITY: usize;

				/// The byte where the feature is set.
				const BYTE_OFFSET: usize = Self::EVEN_BIT / 8;

				/// The bitmask for the feature's required flag relative to the [`BYTE_OFFSET`].
				///
				/// [`BYTE_OFFSET`]: #associatedconstant.BYTE_OFFSET
				const REQUIRED_MASK: u8 = 1 << (Self::EVEN_BIT - 8 * Self::BYTE_OFFSET);

				/// The bitmask for the feature's optional flag relative to the [`BYTE_OFFSET`].
				///
				/// [`BYTE_OFFSET`]: #associatedconstant.BYTE_OFFSET
				const OPTIONAL_MASK: u8 = 1 << (Self::ODD_BIT - 8 * Self::BYTE_OFFSET);

				/// Returns whether the feature is supported by the given flags.
				#[inline]
				fn supports_feature(flags: &Vec<u8>) -> bool {
					flags.len() > Self::BYTE_OFFSET &&
						(flags[Self::BYTE_OFFSET] & (Self::REQUIRED_MASK | Self::OPTIONAL_MASK)) != 0
				}

				/// Sets the feature's optional (odd) bit in the given flags.
				#[inline]
				fn set_optional_bit(flags: &mut Vec<u8>) {
					if flags.len() <= Self::BYTE_OFFSET {
						flags.resize(Self::BYTE_OFFSET + 1, 0u8);
					}

					flags[Self::BYTE_OFFSET] |= Self::OPTIONAL_MASK;
				}

				/// Clears the feature's required (even) and optional (odd) bits from the given
				/// flags.
				#[inline]
				fn clear_bits(flags: &mut Vec<u8>) {
					if flags.len() > Self::BYTE_OFFSET {
						flags[Self::BYTE_OFFSET] &= !Self::REQUIRED_MASK;
						flags[Self::BYTE_OFFSET] &= !Self::OPTIONAL_MASK;
					}
				}
			}

			$(
				impl $feature for $context {
					// EVEN_BIT % 2 == 0
					const ASSERT_EVEN_BIT_PARITY: usize = 0 - (<Self as $feature>::EVEN_BIT % 2);

					// ODD_BIT % 2 == 1
					const ASSERT_ODD_BIT_PARITY: usize = (<Self as $feature>::ODD_BIT % 2) - 1;
				}
			)*
		}
	}

	define_feature!(1, DataLossProtect, [InitContext, NodeContext],
		"Feature flags for `option_data_loss_protect`.");
	// NOTE: Per Bolt #9, initial_routing_sync has no even bit.
	define_feature!(3, InitialRoutingSync, [InitContext],
		"Feature flags for `initial_routing_sync`.");
	define_feature!(5, UpfrontShutdownScript, [InitContext, NodeContext],
		"Feature flags for `option_upfront_shutdown_script`.");
	define_feature!(9, VariableLengthOnion, [InitContext, NodeContext],
		"Feature flags for `var_onion_optin`.");
	define_feature!(15, PaymentSecret, [InitContext, NodeContext],
		"Feature flags for `payment_secret`.");
	define_feature!(17, BasicMPP, [InitContext, NodeContext],
		"Feature flags for `basic_mpp`.");
}

/// Tracks the set of features which a node implements, templated by the context in which it
/// appears.
pub struct Features<T: sealed::Context> {
	/// Note that, for convenience, flags is LITTLE endian (despite being big-endian on the wire)
	flags: Vec<u8>,
	mark: PhantomData<T>,
}

impl<T: sealed::Context> Clone for Features<T> {
	fn clone(&self) -> Self {
		Self {
			flags: self.flags.clone(),
			mark: PhantomData,
		}
	}
}
impl<T: sealed::Context> PartialEq for Features<T> {
	fn eq(&self, o: &Self) -> bool {
		self.flags.eq(&o.flags)
	}
}
impl<T: sealed::Context> fmt::Debug for Features<T> {
	fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
		self.flags.fmt(fmt)
	}
}

/// Features used within an `init` message.
pub type InitFeatures = Features<sealed::InitContext>;
/// Features used within a `node_announcement` message.
pub type NodeFeatures = Features<sealed::NodeContext>;
/// Features used within a `channel_announcement` message.
pub type ChannelFeatures = Features<sealed::ChannelContext>;

impl InitFeatures {
	/// Writes all features present up to, and including, 13.
	pub(crate) fn write_up_to_13<W: Writer>(&self, w: &mut W) -> Result<(), ::std::io::Error> {
		let len = cmp::min(2, self.flags.len());
		w.size_hint(len + 2);
		(len as u16).write(w)?;
		for i in (0..len).rev() {
			if i == 0 {
				self.flags[i].write(w)?;
			} else {
				// On byte 1, we want up-to-and-including-bit-13, 0-indexed, which is
				// up-to-and-including-bit-5, 0-indexed, on this byte:
				(self.flags[i] & 0b00_11_11_11).write(w)?;
			}
		}
		Ok(())
	}

	/// or's another InitFeatures into this one.
	pub(crate) fn or(mut self, o: InitFeatures) -> InitFeatures {
		let total_feature_len = cmp::max(self.flags.len(), o.flags.len());
		self.flags.resize(total_feature_len, 0u8);
		for (byte, o_byte) in self.flags.iter_mut().zip(o.flags.iter()) {
			*byte |= *o_byte;
		}
		self
	}
}

impl<T: sealed::Context> Features<T> {
	/// Create a blank Features with no features set
	pub fn empty() -> Features<T> {
		Features {
			flags: Vec::new(),
			mark: PhantomData,
		}
	}

	/// Creates features known by the implementation as defined by [`T::KNOWN_FEATURE_FLAGS`].
	///
	/// [`T::KNOWN_FEATURE_FLAGS`]: sealed/trait.Context.html#associatedconstant.KNOWN_FEATURE_FLAGS
	pub fn known() -> Features<T> {
		Self {
			flags: T::KNOWN_FEATURE_FLAGS.to_vec(),
			mark: PhantomData,
		}
	}

	/// Takes the flags that we know how to interpret in an init-context features that are also
	/// relevant in a node-context features and creates a node-context features from them.
	/// Be sure to blank out features that are unknown to us.
	pub(crate) fn with_known_relevant_init_flags(init_ctx: &InitFeatures) -> Self {
		let byte_count = T::KNOWN_FEATURE_MASK.len();
		let mut flags = Vec::new();
		for (i, feature_byte) in init_ctx.flags.iter().enumerate() {
			if i < byte_count {
				flags.push(feature_byte & T::KNOWN_FEATURE_MASK[i]);
			}
		}
		Self { flags, mark: PhantomData, }
	}

	#[cfg(test)]
	/// Create a Features given a set of flags, in LE.
	pub fn from_le_bytes(flags: Vec<u8>) -> Features<T> {
		Features {
			flags,
			mark: PhantomData,
		}
	}

	#[cfg(test)]
	/// Gets the underlying flags set, in LE.
	pub fn le_flags(&self) -> &Vec<u8> {
		&self.flags
	}

	pub(crate) fn requires_unknown_bits(&self) -> bool {
		// Bitwise AND-ing with all even bits set except for known features will select required
		// unknown features.
		let byte_count = T::KNOWN_FEATURE_MASK.len();
		self.flags.iter().enumerate().any(|(i, &byte)| {
			let required_features = 0b01_01_01_01;
			let unknown_features = if i < byte_count {
				!T::KNOWN_FEATURE_MASK[i]
			} else {
				0b11_11_11_11
			};
			(byte & (required_features & unknown_features)) != 0
		})
	}

	pub(crate) fn supports_unknown_bits(&self) -> bool {
		// Bitwise AND-ing with all even and odd bits set except for known features will select
		// both required and optional unknown features.
		let byte_count = T::KNOWN_FEATURE_MASK.len();
		self.flags.iter().enumerate().any(|(i, &byte)| {
			let unknown_features = if i < byte_count {
				!T::KNOWN_FEATURE_MASK[i]
			} else {
				0b11_11_11_11
			};
			(byte & unknown_features) != 0
		})
	}

	/// The number of bytes required to represent the feature flags present. This does not include
	/// the length bytes which are included in the serialized form.
	pub(crate) fn byte_count(&self) -> usize {
		self.flags.len()
	}

	#[cfg(test)]
	pub(crate) fn set_require_unknown_bits(&mut self) {
		let newlen = cmp::max(3, self.flags.len());
		self.flags.resize(newlen, 0u8);
		self.flags[2] |= 0x40;
	}

	#[cfg(test)]
	pub(crate) fn clear_require_unknown_bits(&mut self) {
		let newlen = cmp::max(3, self.flags.len());
		self.flags.resize(newlen, 0u8);
		self.flags[2] &= !0x40;
		if self.flags.len() == 3 && self.flags[2] == 0 {
			self.flags.resize(2, 0u8);
		}
		if self.flags.len() == 2 && self.flags[1] == 0 {
			self.flags.resize(1, 0u8);
		}
	}
}

impl<T: sealed::DataLossProtect> Features<T> {
	pub(crate) fn supports_data_loss_protect(&self) -> bool {
		<T as sealed::DataLossProtect>::supports_feature(&self.flags)
	}
}

impl<T: sealed::UpfrontShutdownScript> Features<T> {
	pub(crate) fn supports_upfront_shutdown_script(&self) -> bool {
		<T as sealed::UpfrontShutdownScript>::supports_feature(&self.flags)
	}
	#[cfg(test)]
	pub(crate) fn unset_upfront_shutdown_script(&mut self) {
		<T as sealed::UpfrontShutdownScript>::clear_bits(&mut self.flags)
	}
}

impl<T: sealed::VariableLengthOnion> Features<T> {
	pub(crate) fn supports_variable_length_onion(&self) -> bool {
		<T as sealed::VariableLengthOnion>::supports_feature(&self.flags)
	}
}

impl<T: sealed::InitialRoutingSync> Features<T> {
	pub(crate) fn initial_routing_sync(&self) -> bool {
		<T as sealed::InitialRoutingSync>::supports_feature(&self.flags)
	}
	pub(crate) fn clear_initial_routing_sync(&mut self) {
		<T as sealed::InitialRoutingSync>::clear_bits(&mut self.flags)
	}
}

impl<T: sealed::PaymentSecret> Features<T> {
	#[allow(dead_code)]
	// Note that we never need to test this since what really matters is the invoice - iff the
	// invoice provides a payment_secret, we assume that we can use it (ie that the recipient
	// supports payment_secret).
	pub(crate) fn supports_payment_secret(&self) -> bool {
		<T as sealed::PaymentSecret>::supports_feature(&self.flags)
	}
}

impl<T: sealed::BasicMPP> Features<T> {
	// We currently never test for this since we don't actually *generate* multipath routes.
	#[allow(dead_code)]
	pub(crate) fn supports_basic_mpp(&self) -> bool {
		<T as sealed::BasicMPP>::supports_feature(&self.flags)
	}
}

impl<T: sealed::Context> Writeable for Features<T> {
	fn write<W: Writer>(&self, w: &mut W) -> Result<(), ::std::io::Error> {
		w.size_hint(self.flags.len() + 2);
		(self.flags.len() as u16).write(w)?;
		for f in self.flags.iter().rev() { // Swap back to big-endian
			f.write(w)?;
		}
		Ok(())
	}
}

impl<T: sealed::Context> Readable for Features<T> {
	fn read<R: ::std::io::Read>(r: &mut R) -> Result<Self, DecodeError> {
		let mut flags: Vec<u8> = Readable::read(r)?;
		flags.reverse(); // Swap to little-endian
		Ok(Self {
			flags,
			mark: PhantomData,
		})
	}
}

#[cfg(test)]
mod tests {
	use super::{ChannelFeatures, InitFeatures, NodeFeatures, Features};

	#[test]
	fn sanity_test_our_features() {
		assert!(!ChannelFeatures::known().requires_unknown_bits());
		assert!(!ChannelFeatures::known().supports_unknown_bits());
		assert!(!InitFeatures::known().requires_unknown_bits());
		assert!(!InitFeatures::known().supports_unknown_bits());
		assert!(!NodeFeatures::known().requires_unknown_bits());
		assert!(!NodeFeatures::known().supports_unknown_bits());

		assert!(InitFeatures::known().supports_upfront_shutdown_script());
		assert!(NodeFeatures::known().supports_upfront_shutdown_script());

		assert!(InitFeatures::known().supports_data_loss_protect());
		assert!(NodeFeatures::known().supports_data_loss_protect());

		assert!(InitFeatures::known().supports_variable_length_onion());
		assert!(NodeFeatures::known().supports_variable_length_onion());

		assert!(InitFeatures::known().supports_payment_secret());
		assert!(NodeFeatures::known().supports_payment_secret());

		assert!(InitFeatures::known().supports_basic_mpp());
		assert!(NodeFeatures::known().supports_basic_mpp());

		let mut init_features = InitFeatures::known();
		assert!(init_features.initial_routing_sync());
		init_features.clear_initial_routing_sync();
		assert!(!init_features.initial_routing_sync());
	}

	#[test]
	fn sanity_test_unkown_bits_testing() {
		let mut features = ChannelFeatures::known();
		features.set_require_unknown_bits();
		assert!(features.requires_unknown_bits());
		features.clear_require_unknown_bits();
		assert!(!features.requires_unknown_bits());
	}

	#[test]
	fn test_node_with_known_relevant_init_flags() {
		// Create an InitFeatures with initial_routing_sync supported.
		let init_features = InitFeatures::known();
		assert!(init_features.initial_routing_sync());

		// Attempt to pull out non-node-context feature flags from these InitFeatures.
		let res = NodeFeatures::with_known_relevant_init_flags(&init_features);

		{
			// Check that the flags are as expected: optional_data_loss_protect,
			// option_upfront_shutdown_script, var_onion_optin, payment_secret, and
			// basic_mpp.
			assert_eq!(res.flags.len(), 3);
			assert_eq!(res.flags[0], 0b00100010);
			assert_eq!(res.flags[1], 0b10000010);
			assert_eq!(res.flags[2], 0b00000010);
		}

		// Check that the initial_routing_sync feature was correctly blanked out.
		let new_features: InitFeatures = Features::from_le_bytes(res.flags);
		assert!(!new_features.initial_routing_sync());
	}
}
