#[macro_export]
macro_rules! impl_empty_dispatch {
    ($(($t:ty, $u:ty)),+) => {
        $(
            impl Dispatch<$t, $u> for WindowState {
                fn event(
                    _state: &mut Self,
                    _proxy: &$t,
                    _event: <$t as wayland_client::Proxy>::Event,
                    _data: &$u,
                    _conn: &Connection,
                    _qhandle: &QueueHandle<Self>,
                ) {
                  info!("Implement empty dispatch event for {:?}", stringify!($t));
                }
            }
        )+
    };
}

#[macro_export]
macro_rules! bind_globals {
    ($global_list:expr, $queue_handle:expr, $(($interface:ty, $name:ident, $version:expr)),+) => {
        {
            $(
                let $name: $interface = $global_list.bind($queue_handle, $version, ())
                    .map_err(|e| LayerShikaError::WaylandDispatch(e.to_string()))?;
            )+
            Ok::<($($interface,)+), LayerShikaError>(($($name,)+))
        }
    };
}
