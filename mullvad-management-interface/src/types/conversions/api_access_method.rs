/// Implements conversions for the auxilliary
/// [`crate::types::proto::ApiAccessMethodSettings`] type to the internal
/// [`mullvad_types::access_method::Settings`] data type.
mod settings {
    use crate::types::{proto, FromProtobufTypeError};
    use mullvad_types::access_method;

    impl From<&access_method::Settings> for proto::ApiAccessMethodSettings {
        fn from(settings: &access_method::Settings) -> Self {
            Self {
                api_access_methods: settings
                    .api_access_methods
                    .iter()
                    .map(|method| method.clone().into())
                    .collect(),
            }
        }
    }

    impl From<access_method::Settings> for proto::ApiAccessMethodSettings {
        fn from(settings: access_method::Settings) -> Self {
            proto::ApiAccessMethodSettings::from(&settings)
        }
    }

    impl TryFrom<proto::ApiAccessMethodSettings> for access_method::Settings {
        type Error = FromProtobufTypeError;

        fn try_from(settings: proto::ApiAccessMethodSettings) -> Result<Self, Self::Error> {
            Ok(Self {
                api_access_methods: settings
                    .api_access_methods
                    .iter()
                    .map(access_method::ApiAccessMethod::try_from)
                    .collect::<Result<Vec<access_method::ApiAccessMethod>, _>>()?,
            })
        }
    }

    impl From<access_method::daemon::ApiAccessMethodReplace> for proto::ApiAccessMethodReplace {
        fn from(value: access_method::daemon::ApiAccessMethodReplace) -> Self {
            let api_access_method = value.access_method;
            proto::ApiAccessMethodReplace {
                index: value.index as u32,
                access_method: Some(proto::ApiAccessMethod::from(api_access_method)),
            }
        }
    }

    impl TryFrom<proto::ApiAccessMethodReplace> for access_method::daemon::ApiAccessMethodReplace {
        type Error = FromProtobufTypeError;

        fn try_from(value: proto::ApiAccessMethodReplace) -> Result<Self, Self::Error> {
            Ok(access_method::daemon::ApiAccessMethodReplace {
                index: value.index as usize,
                access_method: value
                    .access_method
                    .ok_or(FromProtobufTypeError::InvalidArgument(
                        "Could not convert Access Method from protobuf",
                    ))
                    .and_then(access_method::ApiAccessMethod::try_from)?,
            })
        }
    }

    impl From<access_method::daemon::ApiAccessMethodToggle> for proto::ApiAccessMethodToggle {
        fn from(value: access_method::daemon::ApiAccessMethodToggle) -> Self {
            let api_access_method = value.access_method;
            let enabled = api_access_method.enabled();
            proto::ApiAccessMethodToggle {
                access_method: Some(proto::ApiAccessMethod::from(api_access_method)),
                enable: enabled,
            }
        }
    }

    impl TryFrom<proto::ApiAccessMethodToggle> for access_method::daemon::ApiAccessMethodToggle {
        type Error = FromProtobufTypeError;

        fn try_from(value: proto::ApiAccessMethodToggle) -> Result<Self, Self::Error> {
            Ok(access_method::daemon::ApiAccessMethodToggle {
                access_method: value
                    .access_method
                    .ok_or(FromProtobufTypeError::InvalidArgument(
                        "Could not convert Access Method from protobuf",
                    ))
                    .and_then(access_method::ApiAccessMethod::try_from)?,
                enable: value.enable,
            })
        }
    }
}

/// Implements conversions for the auxilliary
/// [`crate::types::proto::ApiAccessMethod`] type to the internal
/// [`mullvad_types::access_method::AccessMethod`] data type.
mod data {
    use crate::types::{proto, FromProtobufTypeError};
    use mullvad_types::access_method::{
        AccessMethod, ApiAccessMethod, BuiltInAccessMethod, ObfuscationProtocol, Shadowsocks,
        Socks5, Socks5Local, Socks5Remote,
    };

    impl TryFrom<proto::ApiAccessMethods> for Vec<ApiAccessMethod> {
        type Error = FromProtobufTypeError;

        fn try_from(value: proto::ApiAccessMethods) -> Result<Self, Self::Error> {
            value
                .api_access_methods
                .iter()
                .map(ApiAccessMethod::try_from)
                .collect()
        }
    }

    impl TryFrom<proto::ApiAccessMethod> for ApiAccessMethod {
        type Error = FromProtobufTypeError;

        fn try_from(value: proto::ApiAccessMethod) -> Result<Self, Self::Error> {
            let name = value.name;
            let enabled = value.enabled;
            let access_method =
                value
                    .access_method
                    .ok_or(FromProtobufTypeError::InvalidArgument(
                        "Could not deserialize Access Method from protobuf",
                    ))?;

            let x = match access_method {
                proto::api_access_method::AccessMethod::Direct(
                    proto::api_access_method::Direct {},
                ) => AccessMethod::from(BuiltInAccessMethod::Direct),

                proto::api_access_method::AccessMethod::Bridges(
                    proto::api_access_method::Bridges {},
                ) => AccessMethod::from(BuiltInAccessMethod::Bridge),
                proto::api_access_method::AccessMethod::Socks5local(local) => {
                    let socks = Socks5Local::from_args(
                        local.ip,
                        local.port as u16,
                        local.local_port as u16,
                    )
                    .ok_or(FromProtobufTypeError::InvalidArgument(
                        "Could not parse Socks5 (local) message from protobuf",
                    ))?;
                    AccessMethod::from(socks)
                }

                proto::api_access_method::AccessMethod::Socks5remote(remote) => {
                    let socks = Socks5Remote::from_args(remote.ip, remote.port as u16).ok_or({
                        FromProtobufTypeError::InvalidArgument(
                            "Could not parse Socks5 (remote) message from protobuf",
                        )
                    })?;
                    AccessMethod::from(socks)
                }
                proto::api_access_method::AccessMethod::Shadowsocks(ss) => {
                    let socks =
                        Shadowsocks::from_args(ss.ip, ss.port as u16, ss.cipher, ss.password)
                            .ok_or(FromProtobufTypeError::InvalidArgument(
                                "Could not parse Shadowsocks message from protobuf",
                            ))?;
                    AccessMethod::from(socks)
                }
            };

            Ok(ApiAccessMethod {
                name,
                enabled,
                access_method: x,
            })
        }
    }

    impl From<ApiAccessMethod> for proto::ApiAccessMethod {
        fn from(value: ApiAccessMethod) -> Self {
            let name = value.get_name();
            let enabled = value.enabled();
            let access_method = match value.access_method {
                AccessMethod::Custom(value) => match value.access_method {
                    ObfuscationProtocol::Shadowsocks(ss) => {
                        proto::api_access_method::AccessMethod::Shadowsocks(
                            proto::api_access_method::Shadowsocks {
                                id: value.id,
                                ip: ss.peer.ip().to_string(),
                                port: ss.peer.port() as u32,
                                password: ss.password,
                                cipher: ss.cipher,
                            },
                        )
                    }
                    ObfuscationProtocol::Socks5(Socks5::Local(Socks5Local { peer, port })) => {
                        proto::api_access_method::AccessMethod::Socks5local(
                            proto::api_access_method::Socks5Local {
                                id: value.id,
                                ip: peer.ip().to_string(),
                                port: peer.port() as u32,
                                local_port: port as u32,
                            },
                        )
                    }
                    ObfuscationProtocol::Socks5(Socks5::Remote(Socks5Remote { peer })) => {
                        proto::api_access_method::AccessMethod::Socks5remote(
                            proto::api_access_method::Socks5Remote {
                                id: value.id,
                                ip: peer.ip().to_string(),
                                port: peer.port() as u32,
                            },
                        )
                    }
                },
                AccessMethod::BuiltIn(value) => match value {
                    mullvad_types::access_method::BuiltInAccessMethod::Direct => {
                        proto::api_access_method::AccessMethod::Direct(
                            proto::api_access_method::Direct {},
                        )
                    }
                    mullvad_types::access_method::BuiltInAccessMethod::Bridge => {
                        proto::api_access_method::AccessMethod::Bridges(
                            proto::api_access_method::Bridges {},
                        )
                    }
                },
            };

            proto::ApiAccessMethod {
                name,
                enabled,
                access_method: Some(access_method),
            }
        }
    }

    impl TryFrom<&proto::ApiAccessMethod> for ApiAccessMethod {
        type Error = FromProtobufTypeError;

        fn try_from(value: &proto::ApiAccessMethod) -> Result<Self, Self::Error> {
            ApiAccessMethod::try_from(value.clone())
        }
    }

    impl From<Vec<ApiAccessMethod>> for proto::ApiAccessMethods {
        fn from(value: Vec<ApiAccessMethod>) -> proto::ApiAccessMethods {
            proto::ApiAccessMethods {
                api_access_methods: value.iter().map(|method| method.clone().into()).collect(),
            }
        }
    }
}
