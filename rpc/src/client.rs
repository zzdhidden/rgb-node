// RGB node providing smart contracts functionality for Bitcoin & Lightning.
//
// Written in 2022 by
//     Dr. Maxim Orlovsky <orlovsky@lnp-bp.org>
//
// Copyright (C) 2022 by LNP/BP Standards Association, Switzerland.
//
// You should have received a copy of the MIT License along with this software.
// If not, see <https://opensource.org/licenses/MIT>.

use std::collections::BTreeSet;
use std::thread::sleep;
use std::time::Duration;

use internet2::addr::ServiceAddr;
use internet2::ZmqSocketType;
use lnpbp::chain::Chain;
use microservices::esb::{self, BusId};
use microservices::rpc;
use rgb::schema::TransitionType;
use rgb::{Contract, ContractId, ContractState};

use crate::messages::HelloReq;
use crate::{
    BusMsg, ClientId, ContractReq, ContractValidity, Error, FailureCode, OutpointSelection, RpcMsg,
    ServiceId,
};

// We have just a single service bus (RPC), so we can use any id
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug, Default, Display)]
#[display("RGBRPC")]
struct RpcBus;

impl BusId for RpcBus {
    type Address = ServiceId;
}

type Bus = esb::EndpointList<RpcBus>;

#[repr(C)]
pub struct Client {
    client_id: ClientId,
    user_agent: String,
    network: Chain,
    response_queue: Vec<RpcMsg>,
    esb: esb::Controller<RpcBus, BusMsg, Handler>,
}

impl Client {
    pub fn with(connect: ServiceAddr, user_agent: String, network: Chain) -> Result<Self, Error> {
        use rgb::secp256k1zkp::rand;

        debug!("RPC socket {}", connect);

        debug!("Setting up RPC client...");
        let client_id = rand::random();
        let bus_config = esb::BusConfig::with_addr(
            connect,
            ZmqSocketType::RouterConnect,
            Some(ServiceId::router()),
        );
        let esb = esb::Controller::with(
            map! {
                RpcBus => bus_config
            },
            Handler {
                identity: ServiceId::Client(client_id),
            },
        )?;

        // We have to sleep in order for ZMQ to bootstrap
        sleep(Duration::from_secs_f32(0.1));

        Ok(Self {
            client_id,
            user_agent,
            network,
            response_queue: empty!(),
            esb,
        })
    }

    pub fn client_id(&self) -> ClientId { self.client_id }

    fn request(&mut self, req: impl Into<RpcMsg>) -> Result<(), Error> {
        let req = req.into();
        debug!("Executing {}", req);
        self.esb.send_to(RpcBus, ServiceId::Rgb, BusMsg::Rpc(req))?;
        Ok(())
    }

    fn response(&mut self) -> Result<RpcMsg, Error> {
        loop {
            if let Some(resp) = self.response_queue.pop() {
                return Ok(resp);
            } else {
                for poll in self.esb.recv_poll()? {
                    match poll.request {
                        BusMsg::Rpc(msg) => self.response_queue.push(msg),
                    }
                }
            }
        }
    }
}

impl Client {
    pub fn hello(&mut self) -> Result<bool, Error> {
        self.request(HelloReq {
            user_agent: self.user_agent.clone(),
            network: self.network.clone(),
        })?;
        match self.response()? {
            RpcMsg::Success(_) => Ok(true),
            RpcMsg::Failure(rpc::Failure {
                code: rpc::FailureCode::Other(FailureCode::ChainMismatch),
                ..
            }) => Ok(false),
            resp @ RpcMsg::Failure(_) => Err(resp.failure_to_error().unwrap_err()),
            _ => Err(Error::UnexpectedServerResponse),
        }
    }

    pub fn register_contract(&mut self, contract: Contract) -> Result<ContractValidity, Error> {
        self.request(RpcMsg::AddContract(contract))?;
        match self.response()?.failure_to_error()? {
            RpcMsg::Invalid(status) => Ok(ContractValidity::Invalid(status)),
            RpcMsg::UnresolvedTxids(txids) => Ok(ContractValidity::UnknownTxids(txids)),
            RpcMsg::Success(_) => Ok(ContractValidity::Valid),
            _ => Err(Error::UnexpectedServerResponse),
        }
    }

    pub fn list_contracts(&mut self) -> Result<BTreeSet<ContractId>, Error> {
        self.request(RpcMsg::ListContracts)?;
        match self.response()?.failure_to_error()? {
            RpcMsg::ContractIds(list) => Ok(list),
            _ => Err(Error::UnexpectedServerResponse),
        }
    }

    pub fn contract_state(&mut self, contract_id: ContractId) -> Result<ContractState, Error> {
        self.request(RpcMsg::GetContractState(contract_id))?;
        match self.response()?.failure_to_error()? {
            RpcMsg::ContractState(state) => Ok(state),
            _ => Err(Error::UnexpectedServerResponse),
        }
    }

    pub fn contract(
        &mut self,
        contract_id: ContractId,
        node_types: Vec<TransitionType>,
    ) -> Result<Contract, Error> {
        self.request(RpcMsg::GetContract(ContractReq {
            contract_id,
            include: node_types.into_iter().collect(),
            outpoints: OutpointSelection::All,
        }))?;
        match self.response()?.failure_to_error()? {
            RpcMsg::Contract(contract) => Ok(contract),
            _ => Err(Error::UnexpectedServerResponse),
        }
    }
}

pub struct Handler {
    identity: ServiceId,
}

// Not used in clients
impl esb::Handler<RpcBus> for Handler {
    type Request = BusMsg;
    type Error = esb::Error<ServiceId>;

    fn identity(&self) -> ServiceId { self.identity.clone() }

    fn handle(
        &mut self,
        _: &mut Bus,
        _: RpcBus,
        _: ServiceId,
        _: BusMsg,
    ) -> Result<(), Self::Error> {
        // Cli does not receive replies for now
        Ok(())
    }

    fn handle_err(&mut self, _: &mut Bus, err: esb::Error<ServiceId>) -> Result<(), Self::Error> {
        // We simply propagate the error since it already has been reported
        Err(err.into())
    }
}
