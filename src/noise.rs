use protocol_name::*;
use crypto_types::*;
use handshakestate::*;
use wrappers::rand_wrapper::*;
use wrappers::crypto_wrapper::*;
use cipherstate::*;
use std::ops::DerefMut;

pub trait CryptoResolver {
    fn resolve_rng(&self) -> Option<Box<RandomType>>;
    fn resolve_dh(&self, choice: &DHChoice) -> Option<Box<DhType>>;
    fn resolve_hash(&self, choice: &HashChoice) -> Option<Box<HashType>>;
    fn resolve_cipher(&self, choice: &CipherChoice) -> Option<Box<CipherStateType>>;
}

pub struct DefaultResolver;
impl CryptoResolver for DefaultResolver {
    fn resolve_rng(&self) -> Option<Box<RandomType>> {
        Some(Box::new(RandomOs::default()))
    }

    fn resolve_dh(&self, choice: &DHChoice) -> Option<Box<DhType>> {
        match *choice {
            DHChoice::Curve25519 => Some(Box::new(Dh25519::default())),
            _                    => None,

        }
    }

    fn resolve_hash(&self, choice: &HashChoice) -> Option<Box<HashType>> {
        match *choice {
            HashChoice::SHA256  => Some(Box::new(HashSHA256::default())),
            HashChoice::SHA512  => Some(Box::new(HashSHA512::default())),
            HashChoice::Blake2s => Some(Box::new(HashBLAKE2s::default())),
            HashChoice::Blake2b => Some(Box::new(HashBLAKE2b::default())),
        }
    }

    fn resolve_cipher(&self, choice: &CipherChoice) -> Option<Box<CipherStateType>> {
        match *choice {
            CipherChoice::ChaChaPoly => Some(Box::new(CipherState::<CipherChaChaPoly>::default())),
            CipherChoice::AESGCM     => Some(Box::new(CipherState::<CipherAESGCM>::default())),
        }
    }
}

pub struct NoiseBuilder<'a> {
    params: NoiseParams,           // Deserialized protocol spec
    resolver: Box<CryptoResolver>, // The mapper from protocol choices to crypto implementations
    pub s:  Option<&'a [u8]>,
    pub e:  Option<&'a [u8]>,
    pub rs: Option<Vec<u8>>,
    pub re: Option<Vec<u8>>,
    pub psk: Option<Vec<u8>>,
    pub plog: Option<Vec<u8>>,
}

impl<'a> NoiseBuilder<'a> {
    pub fn new(params: NoiseParams) -> Self {
        Self::with_resolver(params, Box::new(DefaultResolver{}))
    }

    pub fn with_resolver(params: NoiseParams, resolver: Box<CryptoResolver>) -> Self
    {
        NoiseBuilder {
            params: params,
            resolver: resolver,
            s: None,
            e: None,
            rs: None,
            re: None,
            plog: None,
            psk: None,
        }
    }

    pub fn preshared_key(mut self, key: &[u8]) -> Self {
        self.psk = Some(key.to_vec());
        self
    }

    pub fn local_private_key(mut self, key: &'a [u8]) -> Self {
        self.s = Some(key);
        self
    }

    pub fn prologue(mut self, key: &[u8]) -> Self {
        self.plog = Some(key.to_vec());
        self
    }

    pub fn remote_public_key(mut self, pub_key: &[u8]) -> Self {
        self.rs = Some(pub_key.to_vec());
        self
    }

    pub fn build_initiator(self) -> Result<HandshakeState, NoiseError> {
        self.build(true)
    }

    pub fn build_responder(self) -> Result<HandshakeState, NoiseError> {
        self.build(false)
    }

    fn build(self, initiator: bool) -> Result<HandshakeState, NoiseError> {
        if !self.s.is_some() && self.params.handshake.needs_local_static_key(initiator) {
            return Err(NoiseError::InitError("local key needed for chosen handshake pattern"));
        }

        if !self.rs.is_some() && self.params.handshake.need_known_remote_pubkey(initiator) {
            return Err(NoiseError::InitError("remote key needed for chosen handshake pattern"));
        }

        let rng = self.resolver.resolve_rng()
            .ok_or(NoiseError::InitError("no suitable RNG"))?;
        let cipher = self.resolver.resolve_cipher(&self.params.cipher)
            .ok_or(NoiseError::InitError("no suitable cipher implementation"))?;
        let hash = self.resolver.resolve_hash(&self.params.hash)
            .ok_or(NoiseError::InitError("no suitable hash implementation"))?;
        let mut s = self.resolver.resolve_dh(&self.params.dh)
            .ok_or(NoiseError::InitError("no suitable DH implementation"))?;
        let mut e = self.resolver.resolve_dh(&self.params.dh)
            .ok_or(NoiseError::InitError("no suitable DH implementation"))?;
        let cipherstate1 = self.resolver.resolve_cipher(&self.params.cipher)
            .ok_or(NoiseError::InitError("no suitable cipher implementation"))?;
        let cipherstate2 = self.resolver.resolve_cipher(&self.params.cipher)
            .ok_or(NoiseError::InitError("no suitable cipher implementation"))?;

        if let Some(s_key) = self.s {
            s.deref_mut().set(s_key);
        }

        if let Some(e_key) = self.e {
            e.deref_mut().set(e_key);
        }

        let has_s = self.s.is_some();
        let has_e = self.e.is_some();
        let has_rs = self.rs.is_some();
        let has_re = self.re.is_some();
        HandshakeState::new(rng, cipher, hash, s, e,
                            self.rs.unwrap_or_else(|| Vec::new()),
                            self.re.unwrap_or_else(|| Vec::new()),
                            has_s, has_e, has_rs, has_re,
                            initiator,
                            self.params.handshake,
                            &[0u8; 0],
                            None,
                            cipherstate1, cipherstate2)
    }
}

mod tests {
    #[test]
    fn test_builder() {
        let noise = NoiseBuilder::new("Noise_NN_25519_ChaChaPoly_SHA256".parse().unwrap())
            .preshared_key(&[1,1,1,1,1,1,1])
            .prologue(&[2,2,2,2,2,2,2,2])
            .local_private_key(&[0u8; 32])
            .build_initiator().unwrap();
    }

    #[test]
    fn test_builder_bad_spec() {
        let params: Result<NoiseParams, _> = "Noise_NK_25519_ChaChaPoly_BLAH256".parse();

        if let Ok(_) = params {
            panic!("NoiseParams should have failed");
        }
    }

    #[test]
    fn test_builder_missing_prereqs() {
        let noise = NoiseBuilder::new("Noise_NK_25519_ChaChaPoly_SHA256".parse().unwrap())
            .preshared_key(&[1,1,1,1,1,1,1])
            .prologue(&[2,2,2,2,2,2,2,2])
            .local_private_key(&[0u8; 32])
            .build_initiator(); // missing remote key, should result in Err

        if let Ok(_) = noise {
            panic!("builder should have failed on build");
        }
    }
}

