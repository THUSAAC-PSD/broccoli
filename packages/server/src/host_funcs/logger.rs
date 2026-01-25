use extism::host_fn;
use tracing::{info, info_span};

host_fn!(pub log_info(user_data: String; msg: String) -> () {
    let plugin_id_handle = user_data.get()?;
    let plugin_id = plugin_id_handle.lock().map_err(|_| extism::Error::msg("Failed to lock plugin_id"))?;

    let span = info_span!("plugin_log", plugin = %*plugin_id);
    let _enter = span.enter();

    info!("{}", msg);
    Ok(())
});
