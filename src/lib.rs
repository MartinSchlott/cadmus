pub struct Version {
    pub cadmus: String,
    pub ct2rs: String,
    pub ctranslate2: String,
}

pub fn version() -> Version {
    Version {
        cadmus: env!("CARGO_PKG_VERSION").to_string(),
        ct2rs: env!("CADMUS_DEP_CT2RS_VERSION").to_string(),
        ctranslate2: env!("CADMUS_DEP_CTRANSLATE2_VERSION").to_string(),
    }
}

#[cfg(feature = "napi")]
mod napi_bridge {
    use napi_derive::napi;

    #[napi(object)]
    pub struct VersionJs {
        pub cadmus: String,
        // napi-derive auto-camelcases snake_case Rust fields. `ct2rs` would
        // otherwise emit as `ct2Rs` because `2` is treated as a word boundary.
        // Pin the JS-side name to the plan's literal `ct2rs`.
        #[napi(js_name = "ct2rs")]
        pub ct2rs: String,
        pub ctranslate2: String,
    }

    #[napi]
    pub fn version() -> VersionJs {
        let v = super::version();
        VersionJs {
            cadmus: v.cadmus,
            ct2rs: v.ct2rs,
            ctranslate2: v.ctranslate2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_returns_three_string_fields() {
        let v = version();
        assert_eq!(v.cadmus, env!("CARGO_PKG_VERSION"));
        assert!(v.cadmus.starts_with("0.2.0"));
        let _: String = v.ct2rs;
        let _: String = v.ctranslate2;
    }
}
