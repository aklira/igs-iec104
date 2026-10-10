// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Session parameters (IEC 60870-5-104 §5.5 and §9.6): the timeouts t0 to t3,
//! the window sizes k and w, and the role of the station.
//!
//! The default values are the ones of the §9.6 table. The ranges that the
//! standard gives as maximum ranges are errors. The ranges it gives as
//! recommendations are reported by [`LinkConfig::recommendations`] and do not
//! stop a configuration (see PROVENANCE.md D-012).

use std::fmt;
use std::time::Duration;

/// Largest value of t0, t1 and t2, in seconds (§9.6 maximum range).
const MAX_TIMEOUT_SECONDS: u64 = 255;

/// Longest t3 that the standard recommends, in seconds: 48 hours.
const RECOMMENDED_MAX_T3_SECONDS: u64 = 48 * 3600;

/// Largest value of k and of w (§5.5 maximum range: 2^15 - 1).
const MAX_WINDOW: u16 = 32767;

/// The role of the station on a connection. The controlling station starts
/// the data transfer with STARTDT; the controlled station answers.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Role {
    /// The controlling station (the client of the connection).
    Controlling,
    /// The controlled station (the server of the connection).
    Controlled,
}

/// The timeouts and window sizes, as plain values. [`LinkConfig::new`] checks
/// them.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Parameters {
    /// t0: time-out of connection establishment.
    pub t0: Duration,
    /// t1: time-out of sending or testing APDUs.
    pub t1: Duration,
    /// t2: time-out for acknowledgements when there is no data; below t1.
    pub t2: Duration,
    /// t3: time-out for sending test frames in case of a long idle state.
    pub t3: Duration,
    /// k: the maximum number of unacknowledged I frames the sender may have.
    pub k: u16,
    /// w: the latest acknowledgement, after receiving w I frames.
    pub w: u16,
}

impl Default for Parameters {
    /// The default values of IEC 60870-5-104 §9.6: t0 30 s, t1 15 s, t2 10 s,
    /// t3 20 s, k 12, w 8.
    fn default() -> Self {
        Self {
            t0: Duration::from_secs(30),
            t1: Duration::from_secs(15),
            t2: Duration::from_secs(10),
            t3: Duration::from_secs(20),
            k: 12,
            w: 8,
        }
    }
}

/// Why a configuration is rejected.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ConfigError {
    /// A timeout is not a whole number of seconds (the accuracy is 1 s).
    NotWholeSeconds {
        /// The parameter: "t0", "t1", "t2" or "t3".
        parameter: &'static str,
    },
    /// A timeout is outside its range.
    TimeoutOutOfRange {
        /// The parameter: "t0", "t1", "t2" or "t3".
        parameter: &'static str,
        /// The value given.
        value: Duration,
    },
    /// t2 is not below t1.
    T2NotBelowT1 {
        /// The value of t1.
        t1: Duration,
        /// The value of t2.
        t2: Duration,
    },
    /// A window size is outside 1 to 32767.
    WindowOutOfRange {
        /// The parameter: "k" or "w".
        parameter: &'static str,
        /// The value given.
        value: u16,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotWholeSeconds { parameter } => {
                write!(f, "{parameter} is not a whole number of seconds")
            }
            Self::TimeoutOutOfRange { parameter, value } => {
                write!(f, "{parameter} = {}s is out of range", value.as_secs())
            }
            Self::T2NotBelowT1 { t1, t2 } => write!(
                f,
                "t2 ({}s) must be below t1 ({}s)",
                t2.as_secs(),
                t1.as_secs()
            ),
            Self::WindowOutOfRange { parameter, value } => {
                write!(f, "{parameter} = {value} is out of range 1 to 32767")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

/// A value that the standard recommends against but that does not stop the
/// configuration.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Recommendation {
    /// w is above two thirds of k (§5.5: "w should not exceed two-thirds of k").
    WindowAboveTwoThirdsOfK {
        /// The value of k.
        k: u16,
        /// The value of w.
        w: u16,
    },
    /// t3 is longer than the 48 hours that §9.6 recommends.
    LongT3 {
        /// The value of t3.
        t3: Duration,
    },
}

/// A validated configuration: the role of the station and its parameters.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct LinkConfig {
    role: Role,
    parameters: Parameters,
}

impl LinkConfig {
    /// Checks the parameters and builds the configuration.
    ///
    /// Each of t0, t1 and t2 must be a whole number of seconds from 1 to 255.
    /// t3 must be a whole number of seconds, at least 1. t2 must be below t1.
    /// k and w must be from 1 to 32767.
    pub fn new(role: Role, parameters: Parameters) -> Result<Self, ConfigError> {
        check_timeout("t0", parameters.t0, MAX_TIMEOUT_SECONDS)?;
        check_timeout("t1", parameters.t1, MAX_TIMEOUT_SECONDS)?;
        check_timeout("t2", parameters.t2, MAX_TIMEOUT_SECONDS)?;
        // t3 has no maximum in the standard, only a recommendation (see recommendations).
        check_timeout("t3", parameters.t3, u64::MAX)?;
        if parameters.t2 >= parameters.t1 {
            return Err(ConfigError::T2NotBelowT1 {
                t1: parameters.t1,
                t2: parameters.t2,
            });
        }
        check_window("k", parameters.k)?;
        check_window("w", parameters.w)?;
        Ok(Self { role, parameters })
    }

    /// The configuration of the §9.6 default values for `role`.
    pub fn with_defaults(role: Role) -> Self {
        Self {
            role,
            parameters: Parameters::default(),
        }
    }

    /// The role of the station.
    pub fn role(self) -> Role {
        self.role
    }

    /// The timeouts and window sizes.
    pub fn parameters(self) -> Parameters {
        self.parameters
    }

    /// The values the standard recommends against. Empty for the defaults.
    pub fn recommendations(self) -> Vec<Recommendation> {
        let mut found = Vec::new();
        let Parameters { t3, k, w, .. } = self.parameters;
        // w <= 2/3 k, written without a division: 3w <= 2k.
        if u32::from(w).saturating_mul(3) > u32::from(k).saturating_mul(2) {
            found.push(Recommendation::WindowAboveTwoThirdsOfK { k, w });
        }
        if t3.as_secs() > RECOMMENDED_MAX_T3_SECONDS {
            found.push(Recommendation::LongT3 { t3 });
        }
        found
    }
}

/// Checks that a timeout is a whole number of seconds from 1 to `max`.
fn check_timeout(parameter: &'static str, value: Duration, max: u64) -> Result<(), ConfigError> {
    if value.subsec_nanos() != 0 {
        return Err(ConfigError::NotWholeSeconds { parameter });
    }
    let seconds = value.as_secs();
    if seconds < 1 || seconds > max {
        return Err(ConfigError::TimeoutOutOfRange { parameter, value });
    }
    Ok(())
}

fn check_window(parameter: &'static str, value: u16) -> Result<(), ConfigError> {
    if value == 0 || value > MAX_WINDOW {
        return Err(ConfigError::WindowOutOfRange { parameter, value });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(seconds: u64) -> Duration {
        Duration::from_secs(seconds)
    }

    /// The parameters with one field replaced.
    fn with(change: impl FnOnce(&mut Parameters)) -> Parameters {
        let mut parameters = Parameters::default();
        change(&mut parameters);
        parameters
    }

    #[test]
    fn defaults_are_the_values_of_the_standard() {
        let defaults = Parameters::default();
        assert_eq!(defaults.t0, secs(30));
        assert_eq!(defaults.t1, secs(15));
        assert_eq!(defaults.t2, secs(10));
        assert_eq!(defaults.t3, secs(20));
        assert_eq!(defaults.k, 12);
        assert_eq!(defaults.w, 8);
    }

    #[test]
    fn default_configuration_is_valid_and_has_no_recommendation() {
        for role in [Role::Controlling, Role::Controlled] {
            let config = LinkConfig::new(role, Parameters::default()).expect("defaults are valid");
            assert_eq!(config, LinkConfig::with_defaults(role));
            assert_eq!(config.role(), role);
            assert_eq!(config.parameters(), Parameters::default());
            assert!(config.recommendations().is_empty());
        }
    }

    #[test]
    fn boundary_values_are_accepted() {
        for (t, value) in [(1, 1), (255, 255)] {
            let parameters = with(|p| {
                p.t0 = secs(value);
                p.t1 = secs(value.max(2));
                p.t2 = secs(1);
                p.t3 = secs(t);
            });
            assert!(
                LinkConfig::new(Role::Controlled, parameters).is_ok(),
                "value {value}s"
            );
        }
        // t2 just below t1, both at the top of their ranges.
        let parameters = with(|p| {
            p.t1 = secs(255);
            p.t2 = secs(254);
        });
        assert!(LinkConfig::new(Role::Controlling, parameters).is_ok());
        // k and w at their limits.
        for (k, w) in [(1, 1), (32767, 32767)] {
            let parameters = with(|p| {
                p.k = k;
                p.w = w;
            });
            assert!(
                LinkConfig::new(Role::Controlling, parameters).is_ok(),
                "k {k}, w {w}"
            );
        }
    }

    #[test]
    fn timeouts_outside_their_range_are_rejected() {
        let zero = with(|p| p.t0 = secs(0));
        assert_eq!(
            LinkConfig::new(Role::Controlling, zero),
            Err(ConfigError::TimeoutOutOfRange {
                parameter: "t0",
                value: secs(0)
            })
        );
        let too_long = with(|p| p.t0 = secs(256));
        assert_eq!(
            LinkConfig::new(Role::Controlling, too_long),
            Err(ConfigError::TimeoutOutOfRange {
                parameter: "t0",
                value: secs(256)
            })
        );
        let t1_long = with(|p| p.t1 = secs(256));
        assert_eq!(
            LinkConfig::new(Role::Controlling, t1_long),
            Err(ConfigError::TimeoutOutOfRange {
                parameter: "t1",
                value: secs(256)
            })
        );
        let t2_zero = with(|p| p.t2 = secs(0));
        assert_eq!(
            LinkConfig::new(Role::Controlling, t2_zero),
            Err(ConfigError::TimeoutOutOfRange {
                parameter: "t2",
                value: secs(0)
            })
        );
        let t3_zero = with(|p| p.t3 = secs(0));
        assert_eq!(
            LinkConfig::new(Role::Controlling, t3_zero),
            Err(ConfigError::TimeoutOutOfRange {
                parameter: "t3",
                value: secs(0)
            })
        );
    }

    #[test]
    fn timeouts_must_be_whole_seconds() {
        let fraction = with(|p| p.t1 = Duration::from_millis(15_500));
        assert_eq!(
            LinkConfig::new(Role::Controlling, fraction),
            Err(ConfigError::NotWholeSeconds { parameter: "t1" })
        );
        let t3_fraction = with(|p| p.t3 = Duration::from_millis(1));
        assert_eq!(
            LinkConfig::new(Role::Controlling, t3_fraction),
            Err(ConfigError::NotWholeSeconds { parameter: "t3" })
        );
    }

    #[test]
    fn t2_must_be_below_t1() {
        let equal = with(|p| p.t2 = secs(15));
        assert_eq!(
            LinkConfig::new(Role::Controlling, equal),
            Err(ConfigError::T2NotBelowT1 {
                t1: secs(15),
                t2: secs(15)
            })
        );
        let above = with(|p| {
            p.t1 = secs(5);
            p.t2 = secs(10);
        });
        assert_eq!(
            LinkConfig::new(Role::Controlling, above),
            Err(ConfigError::T2NotBelowT1 {
                t1: secs(5),
                t2: secs(10)
            })
        );
    }

    #[test]
    fn windows_outside_1_to_32767_are_rejected() {
        for value in [0, 32768, u16::MAX] {
            let k = with(|p| p.k = value);
            assert_eq!(
                LinkConfig::new(Role::Controlling, k),
                Err(ConfigError::WindowOutOfRange {
                    parameter: "k",
                    value
                })
            );
            let w = with(|p| p.w = value);
            assert_eq!(
                LinkConfig::new(Role::Controlling, w),
                Err(ConfigError::WindowOutOfRange {
                    parameter: "w",
                    value
                })
            );
        }
    }

    #[test]
    fn window_above_two_thirds_of_k_is_a_recommendation_not_an_error() {
        // 3 * 9 = 27 > 2 * 12 = 24: w is above two thirds of k.
        let parameters = with(|p| p.w = 9);
        let config = LinkConfig::new(Role::Controlling, parameters).expect("accepted");
        assert_eq!(
            config.recommendations(),
            vec![Recommendation::WindowAboveTwoThirdsOfK { k: 12, w: 9 }]
        );
        // 3 * 8 = 24 = 2 * 12: exactly two thirds, no recommendation.
        let at_limit = LinkConfig::new(Role::Controlling, with(|p| p.w = 8)).expect("accepted");
        assert!(at_limit.recommendations().is_empty());
    }

    #[test]
    fn t3_above_48_hours_is_a_recommendation_not_an_error() {
        let parameters = with(|p| p.t3 = secs(48 * 3600 + 1));
        let config = LinkConfig::new(Role::Controlled, parameters).expect("accepted");
        assert_eq!(
            config.recommendations(),
            vec![Recommendation::LongT3 {
                t3: secs(48 * 3600 + 1)
            }]
        );
        let at_limit =
            LinkConfig::new(Role::Controlled, with(|p| p.t3 = secs(48 * 3600))).expect("accepted");
        assert!(at_limit.recommendations().is_empty());
    }

    #[test]
    fn error_messages_name_the_parameter() {
        let error = ConfigError::T2NotBelowT1 {
            t1: secs(15),
            t2: secs(15),
        };
        assert_eq!(error.to_string(), "t2 (15s) must be below t1 (15s)");
        let error = ConfigError::WindowOutOfRange {
            parameter: "k",
            value: 0,
        };
        assert_eq!(error.to_string(), "k = 0 is out of range 1 to 32767");
    }
}
