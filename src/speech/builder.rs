//! Configurator and builder for speech or intent recognition.

use super::{
    audio::{Audio, AudioSpec},
    events::Flags,
    recognizer::{IntentTrigger, Model, Recognizer},
    synthesizer::*,
};
use crate::{
    hr,
    properties::{Properties, PropertyBag},
    speech_api::*,
    DeriveHandle, FlattenProps, Handle, Result, INVALID_HANDLE,
};
use serde::{Deserialize, Serialize};
use std::ffi::CString;

/// Generate getter and setter for plain attribute with type conversition.
macro_rules! DefineAttribute {
    ($name:ident, $setter:ident, $t:ty) => (
        pub fn $name(&self) -> &$t {
            &self.$name
        }
        pub fn $setter<T: Into<$t>>(&mut self, v: T) -> &mut Self {
            self.$name = v.into();
            self
        }
    )
}

/// Declare for simple attribute with Copy trait.
macro_rules! SimpleAttribute {
    ($name:ident, $setter:ident, $t:ty) => (
        pub fn $name(&self) -> &$t {
            &self.$name
        }
        pub fn $setter(&mut self, v: $t) -> &mut Self {
            self.$name = v;
            self
        }
    )
}

/// Shortcut for common properties. The setter name with 'put_' prefix while the simple attribute with 'set_' prefix.
macro_rules! DefineProperty {
    ($getter:ident, $setter:ident, $id:expr) => (
        pub fn $getter(&self) -> Result<String> {
            self.get_by_id($id)
        }

        pub fn $setter<T: ToString>(&mut self, v: T) -> Result<&mut Self> {
            self.put_by_id($id, v)?;
            Ok(self)
        }
    )
}

DeriveHandle!(
    RecognizerConfig,
    SPXSPEECHCONFIGHANDLE,
    speech_config_release,
    speech_config_is_handle_valid
);

/// Configurator.
pub struct RecognizerConfig {
    flags: Flags,
    audio_spec: Option<AudioSpec>,
    audio_file_path: String,
    pull_mode: bool,
    model_id: String,
    intents: Vec<String>,
    target_languages: Vec<String>,
    timeout: u32,
    handle: SPXSPEECHCONFIGHANDLE,
    props: Properties,
}

impl RecognizerConfig {
    fn new(handle: SPXSPEECHCONFIGHANDLE) -> Result<Self> {
        let mut hprops = INVALID_HANDLE;
        hr!(speech_config_get_property_bag(handle, &mut hprops))?;
        Ok(RecognizerConfig {
            handle,
            props: Properties::new(hprops),
            flags: Flags::Recognized,
            audio_spec: None,
            audio_file_path: String::new(),
            pull_mode: false,
            model_id: String::new(),
            intents: Vec::new(),
            target_languages: Vec::new(),
            timeout: UINT32_MAX,
        })
    }

    /// Initiate with subscription key and region. If region is empty, use the default region "westus".
    pub fn from_subscription(subscription: &str, region: &str) -> Result<Self> {
        let mut handle = INVALID_HANDLE;
        let subscription = CString::new(subscription)?;
        let region = CString::new(region)?;
        hr!(speech_config_from_subscription(
            &mut handle,
            subscription.as_ptr(),
            region.as_ptr(),
        ))?;
        RecognizerConfig::new(handle)
    }

    /// Creates an instance of the speech config with specified authorization token and region.
    /// Note: The caller needs to ensure that the authorization token is valid. Before the authorization token expires, the caller needs to refresh it by calling this setter with a new valid token.
    /// As configuration values are copied when creating a new recognizer, the new token value will not apply to recognizers that have already been created.
    /// For recognizers that have been created before, you need to set authorization token of the corresponding recognizer to refresh the token. Otherwise, the recognizers will encounter errors during recognition.
    pub fn from_authorization_token(token: &str, region: &str) -> Result<Self> {
        let mut handle = INVALID_HANDLE;
        let token = CString::new(token)?;
        let region = CString::new(region)?;
        hr!(speech_config_from_authorization_token(
            &mut handle,
            token.as_ptr(),
            region.as_ptr(),
        ))?;
        RecognizerConfig::new(handle)
    }

    /// Creates an instance of the speech config with specified endpoint and subscription.
    /// This method is intended only for users who use a non-standard service endpoint.
    /// Note: The query parameters specified in the endpoint URL are not changed, even if they are set by any other APIs.
    /// For example, if language is defined in uri as query parameter "language=de-DE", and also set by CreateSpeechRecognizer("en-US"), the language setting in uri takes precedence, and the effective language is "de-DE".
    /// Only the parameters that are not specified in the endpoint URL can be set by other APIs.
    /// Note: To use authorization token with FromEndpoint, pass an empty string to the subscription in the FromEndpoint method, and then call SetAuthorizationToken() on the created SpeechConfig instance to use the authorization token.
    pub fn from_endpoint(endpoint: &str, subscription: &str) -> Result<Self> {
        let mut handle = INVALID_HANDLE;
        let endpoint = CString::new(endpoint)?;
        let subscription = CString::new(subscription)?;
        hr!(speech_config_from_endpoint(
            &mut handle,
            endpoint.as_ptr(),
            subscription.as_ptr(),
        ))?;
        RecognizerConfig::new(handle)
    }

    /// Generate a simple speech recognizer.
    pub fn recognizer(&self) -> Result<Recognizer> {
        let audio = self.audio_input()?;
        let mut rh = INVALID_HANDLE;
        hr!(recognizer_create_speech_recognizer_from_config(
            &mut rh,
            self.handle,
            audio.handle(),
        ))?;
        Ok(Recognizer::new(
            rh,
            audio,
            self.flags | Flags::Speech,
            self.timeout,
        ))
    }

    /// Generate a simple speech recognizer.
    pub fn synthesizer(&self) -> Result<Synthesizer> {
        let audio = self.audio_output()?;
        let mut sh = INVALID_HANDLE;
        hr!(synthesizer_create_speech_synthesizer_from_config(
            &mut sh,
            self.handle,
            audio.handle(),
        ))?;
        Synthesizer::new(sh, audio, self.flags | Flags::Synthesis)
    }

    /// Generate a recognizer with speech and intent recognition.
    pub fn intent_recognizer(&self) -> Result<Recognizer> {
        let audio = self.audio_input()?;
        let mut rh = INVALID_HANDLE;
        hr!(recognizer_create_intent_recognizer_from_config(
            &mut rh,
            self.handle,
            audio.handle(),
        ))?;

        let reco = Recognizer::new(
            rh,
            audio,
            self.flags | Flags::Intent,
            self.timeout,
        );
        self.apply_intents(&reco)?;
        Ok(reco)
    }

    /// Generate a recognizer with speech and intent recognition.
    pub fn translator(&self) -> Result<Recognizer> {
        self.apply_target_languages()?;
        let audio = self.audio_input()?;
        let mut rh = INVALID_HANDLE;
        hr!(recognizer_create_translation_recognizer_from_config(
            &mut rh,
            self.handle,
            audio.handle(),
        ))?;

        let reco = Recognizer::new(
            rh,
            audio,
            self.flags | Flags::Translation,
            self.timeout,
        );
        Ok(reco)
    }

    /// Create audio input object.
    pub fn audio_input(&self) -> Result<Audio> {
        if !self.audio_file_path.is_empty() {
            Audio::create_input_from_wav_file(&self.audio_file_path)
        } else if let Some(ref cfg) = self.audio_spec {
            if self.pull_mode {
                Audio::create_pull_input(cfg)
            } else {
                Audio::create_push_input(cfg)
            }
        } else {
            Audio::create_input_from_microphone()
        }
    }

    /// Create audio output object.
    pub fn audio_output(&self) -> Result<Audio> {
        if !self.audio_file_path.is_empty() {
            Audio::create_output_to_file(&self.audio_file_path)
        } else if let Some(ref cfg) = self.audio_spec {
            Audio::create_output(cfg)
        } else {
            Audio::create_output_to_speaker()
        }
    }

    /// Get auido input configration. Return None if input of microphone.
    pub fn audio_spec(&self) -> Option<AudioSpec> {
        self.audio_spec
    }

    /// Set audio input configuration. Type of T should be tuple(rate, bits, channels) or AudioConfig.
    pub fn set_audio_spec<T: Into<AudioSpec>>(&mut self, v: T) -> &mut Self {
        self.audio_spec = Some(v.into());
        self
    }

    /// Streaming mode of audio input. Pull mode is true, push mode is false.
    SimpleAttribute!(pull_mode, set_pull_mode, bool);
    /// Bitmask flags for events handlers.
    SimpleAttribute!(flags, set_flags, Flags);
    /// Timeout value for aynchronous operation.
    SimpleAttribute!(timeout, set_timeout, u32);
    /// If audio file path is provided, audio input is the single file.
    DefineAttribute!(audio_file_path, set_audio_file_path, String);
    /// Language understanding model application id.
    DefineAttribute!(model_id, set_model_id, String);

    /// If intents is empty, all the intents of the given model will be loaded.
    /// If model is not set, content of intents is a set of phrases for simple intent matching.
    DefineAttribute!(intents, set_intents, Vec<String>);
    /// Shortcut of intents vector operation.
    pub fn add_intent(&mut self, name: &str) -> Result<&mut Self> {
        self.intents.push(name.to_string());
        Ok(self)
    }
    /// Add intents from configuration to generated recognizer.
    fn apply_intents(&self, reco: &Recognizer) -> Result {
        if self.model_id.is_empty() {
            for ref phrase in &self.intents {
                let trigger = IntentTrigger::from_phrase(phrase)?;
                reco.add_intent(phrase, &trigger)?;
            }
            return Ok(());
        }

        let model = Model::from_app_id(&self.model_id)?;
        if self.intents.is_empty() {
            let trigger = IntentTrigger::from_model_all(&model)?;
            return reco.add_intent("", &trigger);
        }

        for ref intent in &self.intents {
            let trigger = IntentTrigger::from_model(&model, intent)?;
            reco.add_intent(intent, &trigger)?;
        }

        Ok(())
    }

    /// Can translate one speech source to multiple languages simultaneously.
    DefineAttribute!(target_languages, set_target_languages, Vec<String>);
    /// Shortcut of intents vector operation.
    pub fn add_target_language(&mut self, name: &str) -> Result<&mut Self> {
        self.target_languages.push(name.to_string());
        Ok(self)
    }
    /// Apply target languages to generate translator.
    fn apply_target_languages(&self) -> Result {
        let tl = self.target_languages.join(",");
        self.props.put_by_id(
            PropertyId_SpeechServiceConnection_TranslationToLanguages,
            tl,
        )
    }

    /// The input language of the speech recognizer.
    DefineProperty!(
        trans_voice_name,
        put_trans_voice_name,
        PropertyId_SpeechServiceConnection_TranslationVoice
    );

    /// The input language of the speech recognizer.
    DefineProperty!(
        synth_voice_name,
        put_synth_voice_name,
        PropertyId_SpeechServiceConnection_SynthVoice
    );

    /// The input language of the speech recognizer.
    DefineProperty!(
        translation_features,
        put_translation_features,
        PropertyId_SpeechServiceConnection_TranslationFeatures
    );

    /// The input language of the speech recognizer.
    DefineProperty!(
        language,
        put_language,
        PropertyId_SpeechServiceConnection_RecoLanguage
    );

    /// The input language of the speech recognizer.
    DefineProperty!(
        synth_language,
        put_synth_language,
        PropertyId_SpeechServiceConnection_SynthLanguage
    );

    /// The endpoint ID of the speech recognizer.
    DefineProperty!(
        endpoint,
        put_endpoint,
        PropertyId_SpeechServiceConnection_EndpointId
    );

    /// Detailed output format or not.
    DefineProperty!(
        detailed_result,
        put_detailed_result,
        PropertyId_SpeechServiceResponse_RequestDetailedResultTrueFalse
    );

    /// Subset of proxy configuration
    pub fn put_proxy(&mut self, proxy: &ProxyConfig) -> Result<&mut Self> {
        self.props.put_by_id(
            PropertyId_SpeechServiceConnection_ProxyHostName,
            &proxy.host_name,
        )?;
        self.props.put_by_id(
            PropertyId_SpeechServiceConnection_ProxyPort,
            &proxy.port.to_string(),
        )?;
        self.props.put_by_id(
            PropertyId_SpeechServiceConnection_ProxyUserName,
            &proxy.user_name,
        )?;
        self.props.put_by_id(
            PropertyId_SpeechServiceConnection_ProxyPassword,
            &proxy.password,
        )?;

        Ok(self)
    }

    pub fn put_synth_audio_format(&mut self, mode: u32) -> Result<&mut Self> {
        hr!(speech_config_set_audio_output_format(self.handle, mode))?;
        Ok(self)
    }
}

FlattenProps!(RecognizerConfig);

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProxyConfig {
    host_name: String,
    port: u32,
    user_name: String,
    password: String,
}
