from pumpkin_api import Plugin, context, logging, metadata, register_plugin

PluginMetadata = metadata.PluginMetadata

class MyPlugin(Plugin):
    def metadata(self) -> PluginMetadata:
        return PluginMetadata(
            name="PumpkinPAPI",
            version="1.0.0",
            authors=["Epix Development"],
            description="Placeholder plugin for PumpkinMC",
            dependencies=[],
            permissions=[]
        )

    def on_load(self, ctx: context.Context) -> None:
        logging.log(logging.Level.INFO, "Plugin loaded!")

register_plugin(MyPlugin)