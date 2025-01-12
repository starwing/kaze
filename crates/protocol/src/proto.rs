include!("proto/kaze.rs");

impl Hdr {
    pub fn seq(&self) -> Option<u32> {
        use hdr::RpcType;
        match self.rpc_type {
            Some(RpcType::Req(seq)) => Some(seq),
            Some(RpcType::Rsp(seq)) => Some(seq),
            Some(RpcType::Ntf(seq)) => Some(seq),
            _ => None,
        }
    }

    pub fn is_req(&self) -> bool {
        match self.rpc_type {
            Some(hdr::RpcType::Req(_)) => true,
            _ => false,
        }
    }

    pub fn is_rsp(&self) -> bool {
        match self.rpc_type {
            Some(hdr::RpcType::Rsp(_)) => true,
            _ => false,
        }
    }

    pub fn is_ntf(&self) -> bool {
        match self.rpc_type {
            Some(hdr::RpcType::Ntf(_)) => true,
            _ => false,
        }
    }
}
