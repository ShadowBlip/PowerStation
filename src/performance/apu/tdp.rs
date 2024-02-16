
pub enum TDPError {
    FeatureUnsupported
}

impl Into<String> for TDPError {
    fn into(self) -> std::string::String { todo!() }
}

pub type TDPResult<T> = Result<T, TDPError>;

pub trait TDPDevice : Sync + Send {

    fn get_ppt_fppt(&self) -> TDPResult<u64>;

    fn set_ppt_fppt(&self) -> TDPResult<()>;

}