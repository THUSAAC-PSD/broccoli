use extism::host_fn;

host_fn!(pub log_info(user_data: String; msg: String) -> () {
    let plugin_id_handle = user_data.get()?;
    let plugin_id = plugin_id_handle.lock().map_err(|_| extism::Error::msg("Failed to lock plugin_id"))?;
    let span = super::host_fn_span("log_info", plugin_id.as_str());
    let _enter = span.enter();
    tracing::info!(plugin = %*plugin_id, "{}", msg);
    Ok(())
});
