use std::net::{IpAddr, SocketAddr};
use std::ops::{Deref, DerefMut};

use ctrlc::set_handler;
pub use simple_socket::PostServing;

use n3_machine::HostMachine;
use n3_net_protocol::{Request, Response, PORT};

pub type SocketServer = simple_socket::SocketServer<Request, Response>;

pub(crate) trait Handle<H>
where
    H: Deref<Target = HostMachine> + DerefMut,
{
    fn handle(self, host: &mut H) -> Response;
}

impl<H> Handle<H> for Request
where
    H: Deref<Target = HostMachine> + DerefMut,
{
    fn handle(self, host: &mut H) -> Response {
        match self {
            Self::Load { job, query } => host
                .load(job, query)
                .map(|num_machines| Response::Load { num_machines }),
            Self::Spawn {
                job,
                id_primaries,
                id_local,
                id_world,
                program,
                command,
            } => host
                .spawn(job, id_primaries, id_local, id_world, &program, &command)
                .map(|()| Response::Awk),
            Self::Join { job } => host.join(job).map(|()| Response::Awk),
            Self::Terminate { job } => host.terminate(job).map(|()| Response::Awk),
        }
        // error handler
        .map_or_else(
            |e| Response::Error {
                message: format!("{:?}", e),
            },
            |x| x,
        )
    }
}

pub fn run_server<H, P>(mut host: H, ip: IpAddr, post: P)
where
    H: Deref<Target = HostMachine> + DerefMut,
    P: Fn(&mut SocketServer) -> PostServing,
{
    let socket = SocketAddr::new(ip, PORT);

    let backlog = Default::default();
    let server = SocketServer::try_new(socket, backlog).unwrap();

    let handler = host.handler.clone();
    set_handler(move || handler.set(false)).unwrap();

    let handler = host.handler.clone();
    server
        .run(
            |x| Handle::<H>::handle(x, &mut host),
            |s| {
                if handler.is_running() {
                    post(s)
                } else {
                    PostServing::Stop
                }
            },
        )
        .unwrap()
}
