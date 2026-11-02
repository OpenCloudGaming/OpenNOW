package com.opencloudgaming.opennow

/** Output boundary for finger mouse gestures; button ownership stays in NativeStreamClient. */
internal interface TouchMouseOutput {
    val settings: StreamSettings
    fun setTouchMouseButton(pressed: Boolean): Boolean
    fun setDirectClickMouseButton(pressed: Boolean): Boolean
    fun sendDirectClickMousePosition(position: AbsoluteMousePosition): Boolean
    fun sendDirectClickMouseClick(position: AbsoluteMousePosition)
    fun sendRawMouseMove(dx: Int, dy: Int): Boolean
    fun sendTouchMouseClick()
    fun sendTouchMouseRightClick()
    fun sendTouchMouseWheel(delta: Int)
}

internal class NativeTouchMouseOutput(private val client: NativeStreamClient) : TouchMouseOutput {
    override val settings: StreamSettings get() = client.settings
    override fun setTouchMouseButton(pressed: Boolean) = client.setTouchMouseButton(pressed)
    override fun setDirectClickMouseButton(pressed: Boolean) = client.setDirectClickMouseButton(pressed)
    override fun sendDirectClickMousePosition(position: AbsoluteMousePosition) = client.sendDirectClickMousePosition(position)
    override fun sendDirectClickMouseClick(position: AbsoluteMousePosition) = client.sendDirectClickMouseClick(position)
    override fun sendRawMouseMove(dx: Int, dy: Int) = client.sendRawMouseMove(dx, dy)
    override fun sendTouchMouseClick() = client.sendTouchMouseClick()
    override fun sendTouchMouseRightClick() = client.sendTouchMouseRightClick()
    override fun sendTouchMouseWheel(delta: Int) = client.sendTouchMouseWheel(delta)
}
