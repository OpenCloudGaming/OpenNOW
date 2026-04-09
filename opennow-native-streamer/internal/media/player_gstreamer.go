//go:build gstreamer

package media

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"sync"
	"time"

	"github.com/OpenCloudGaming/OpenNOW/opennow-native-streamer/pkg/protocol"
	"github.com/go-gst/go-gst/gst"
	"github.com/go-gst/go-gst/gst/app"
	"github.com/veandco/go-sdl2/sdl"
)

const (
	gamepadDpadUp    = 0x0001
	gamepadDpadDown  = 0x0002
	gamepadDpadLeft  = 0x0004
	gamepadDpadRight = 0x0008
	gamepadStart     = 0x0010
	gamepadBack      = 0x0020
	gamepadLS        = 0x0040
	gamepadRS        = 0x0080
	gamepadLB        = 0x0100
	gamepadRB        = 0x0200
	gamepadGuide     = 0x0400
	gamepadA         = 0x1000
	gamepadB         = 0x2000
	gamepadX         = 0x4000
	gamepadY         = 0x8000
)

type controllerState struct {
	buttons      uint16
	leftTrigger  uint8
	rightTrigger uint8
	leftStickX   int16
	leftStickY   int16
	rightStickX  int16
	rightStickY  int16
	connected    bool
}

type gstreamerPlayer struct {
	mu          sync.Mutex
	pipe        *gst.Pipeline
	videoIn     *app.Source
	audioIn     *app.Source
	window      *sdl.Window
	cancel      context.CancelFunc
	controllers map[uint32]*controllerState
	inputSink   func(protocol.InputMessage) error
}

func init() {
	RegisterFactory(func() Player { return &gstreamerPlayer{} })
}

func (p *gstreamerPlayer) Start(ctx context.Context, cfg Config) error {
	if err := gst.Init(nil); err != nil {
		return err
	}
	if err := sdl.Init(sdl.INIT_VIDEO | sdl.INIT_GAMECONTROLLER | sdl.INIT_AUDIO); err != nil {
		return err
	}
	p.inputSink = cfg.InputSink
	p.controllers = map[uint32]*controllerState{}
	if _, err := sdl.GameControllerEventState(sdl.ENABLE); err != nil {
		return err
	}
	window, err := sdl.CreateWindow(cfg.WindowTitle, sdl.WINDOWPOS_CENTERED, sdl.WINDOWPOS_CENTERED, int32(cfg.Width), int32(cfg.Height), sdl.WINDOW_RESIZABLE|sdl.WINDOW_ALLOW_HIGHDPI)
	if err != nil {
		return err
	}
	p.window = window

	pipe, videoIn, audioIn, err := buildPipeline(cfg.Codec)
	if err != nil {
		return err
	}
	p.pipe = pipe
	p.videoIn = videoIn
	p.audioIn = audioIn
	if _, err = pipe.SetState(gst.StatePlaying); err != nil {
		return err
	}

	loopCtx, cancel := context.WithCancel(ctx)
	p.cancel = cancel
	go p.eventLoop(loopCtx)
	return nil
}

func buildPipeline(codec string) (*gst.Pipeline, *app.Source, *app.Source, error) {
	videoCandidates := videoPipelineCandidates(codec)
	audioPart := "appsrc name=audioIn is-live=true format=time do-timestamp=true ! application/x-rtp,media=audio,encoding-name=OPUS,payload=111,clock-rate=48000 ! queue leaky=downstream max-size-buffers=8 ! rtpopusdepay ! opusdec ! audioconvert ! audioresample ! autoaudiosink sync=false"
	var lastErr error
	for _, videoPart := range videoCandidates {
		launch := fmt.Sprintf("%s %s", videoPart, audioPart)
		obj, err := gst.NewPipelineFromString(launch)
		if err != nil {
			lastErr = err
			continue
		}
		pipe, ok := obj.(*gst.Pipeline)
		if !ok {
			lastErr = errors.New("unexpected pipeline type")
			continue
		}
		videoElem, err := pipe.GetElementByName("videoIn")
		if err != nil {
			lastErr = err
			continue
		}
		audioElem, err := pipe.GetElementByName("audioIn")
		if err != nil {
			lastErr = err
			continue
		}
		return pipe, app.SrcFromElement(videoElem), app.SrcFromElement(audioElem), nil
	}
	if lastErr == nil {
		lastErr = fmt.Errorf("no usable GStreamer pipeline for codec %s", codec)
	}
	return nil, nil, nil, lastErr
}

func videoPipelineCandidates(codec string) []string {
	switch codec {
	case "H265":
		return []string{
			"appsrc name=videoIn is-live=true format=time do-timestamp=true ! application/x-rtp,media=video,encoding-name=H265,payload=96,clock-rate=90000 ! queue leaky=downstream max-size-buffers=4 ! rtph265depay ! h265parse ! avdec_h265 ! videoconvert ! autovideosink sync=false",
			"appsrc name=videoIn is-live=true format=time do-timestamp=true ! application/x-rtp,media=video,encoding-name=H265,payload=96,clock-rate=90000 ! queue leaky=downstream max-size-buffers=4 ! rtph265depay ! decodebin ! videoconvert ! autovideosink sync=false",
		}
	case "AV1":
		return []string{
			"appsrc name=videoIn is-live=true format=time do-timestamp=true ! application/x-rtp,media=video,encoding-name=AV1,payload=96,clock-rate=90000 ! queue leaky=downstream max-size-buffers=4 ! rtpav1depay ! av1parse ! dav1ddec ! videoconvert ! autovideosink sync=false",
			"appsrc name=videoIn is-live=true format=time do-timestamp=true ! application/x-rtp,media=video,encoding-name=AV1,payload=96,clock-rate=90000 ! queue leaky=downstream max-size-buffers=4 ! rtpav1depay ! decodebin ! videoconvert ! autovideosink sync=false",
		}
	default:
		return []string{
			"appsrc name=videoIn is-live=true format=time do-timestamp=true ! application/x-rtp,media=video,encoding-name=H264,payload=96,clock-rate=90000 ! queue leaky=downstream max-size-buffers=4 ! rtph264depay ! h264parse ! avdec_h264 ! videoconvert ! autovideosink sync=false",
			"appsrc name=videoIn is-live=true format=time do-timestamp=true ! application/x-rtp,media=video,encoding-name=H264,payload=96,clock-rate=90000 ! queue leaky=downstream max-size-buffers=4 ! rtph264depay ! decodebin ! videoconvert ! autovideosink sync=false",
		}
	}
}

func (p *gstreamerPlayer) PushVideoRTP(packet []byte) error {
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.videoIn == nil {
		return nil
	}
	buf := gst.NewBufferFromBytes(packet)
	return p.videoIn.PushBuffer(buf)
}

func (p *gstreamerPlayer) PushAudioRTP(packet []byte) error {
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.audioIn == nil {
		return nil
	}
	buf := gst.NewBufferFromBytes(packet)
	return p.audioIn.PushBuffer(buf)
}

func (p *gstreamerPlayer) SetStatus(status string) {
	if p.window != nil {
		p.window.SetTitle(fmt.Sprintf("%s — %s", protocol.WindowTitle, status))
	}
}

func (p *gstreamerPlayer) Close() error {
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.cancel != nil {
		p.cancel()
		p.cancel = nil
	}
	if p.pipe != nil {
		p.pipe.SetState(gst.StateNull)
		p.pipe = nil
	}
	if p.window != nil {
		p.window.Destroy()
		p.window = nil
	}
	sdl.Quit()
	return nil
}

func (p *gstreamerPlayer) eventLoop(ctx context.Context) {
	ticker := time.NewTicker(time.Second / 120)
	defer ticker.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
			for event := sdl.PollEvent(); event != nil; event = sdl.PollEvent() {
				switch typed := event.(type) {
				case *sdl.QuitEvent:
					return
				case *sdl.KeyboardEvent:
					p.handleKeyboardEvent(typed)
				case *sdl.MouseMotionEvent:
					p.handleMouseMotionEvent(typed)
				case *sdl.MouseButtonEvent:
					p.handleMouseButtonEvent(typed)
				case *sdl.MouseWheelEvent:
					p.handleMouseWheelEvent(typed)
				case *sdl.ControllerDeviceAddedEvent:
					p.handleControllerAdded(typed)
				case *sdl.ControllerDeviceRemovedEvent:
					p.handleControllerRemoved(typed)
				case *sdl.ControllerButtonEvent:
					p.handleControllerButtonEvent(typed)
				case *sdl.ControllerAxisEvent:
					p.handleControllerAxisEvent(typed)
				}
			}
		}
	}
}

func (p *gstreamerPlayer) handleKeyboardEvent(event *sdl.KeyboardEvent) {
	mapped := mapSDLKeyboardEvent(event)
	if mapped == nil {
		return
	}
	down := event.Type == sdl.KEYDOWN
	_ = p.emitInput("keyboard", protocol.KeyboardInput{
		Keycode:   mapped.keycode,
		Scancode:  mapped.scancode,
		Modifiers: mapped.modifiers,
		Down:      down,
	})
}

func (p *gstreamerPlayer) handleMouseMotionEvent(event *sdl.MouseMotionEvent) {
	_ = p.emitInput("mouse-move", protocol.MouseMoveInput{DX: clampInt16(int(event.XRel)), DY: clampInt16(int(event.YRel))})
}

func (p *gstreamerPlayer) handleMouseButtonEvent(event *sdl.MouseButtonEvent) {
	button := mapSDLMouseButton(event.Button)
	if button == 0 {
		return
	}
	_ = p.emitInput("mouse-button", protocol.MouseButtonInput{Button: button, Down: event.Type == sdl.MOUSEBUTTONDOWN})
}

func (p *gstreamerPlayer) handleMouseWheelEvent(event *sdl.MouseWheelEvent) {
	_ = p.emitInput("mouse-wheel", protocol.MouseWheelInput{Delta: clampInt16(int(event.Y * 120))})
}

func (p *gstreamerPlayer) handleControllerAdded(event *sdl.ControllerDeviceAddedEvent) {
	controller, err := sdl.GameControllerOpen(int(event.Which))
	if err != nil {
		return
	}
	defer controller.Close()
	instanceID := uint32(controller.Joystick().InstanceID())
	state := p.ensureControllerState(instanceID)
	state.connected = true
	p.emitControllerState(instanceID, state)
}

func (p *gstreamerPlayer) handleControllerRemoved(event *sdl.ControllerDeviceRemovedEvent) {
	instanceID := uint32(event.Which)
	state := p.ensureControllerState(instanceID)
	state.connected = false
	state.buttons = 0
	state.leftTrigger = 0
	state.rightTrigger = 0
	state.leftStickX = 0
	state.leftStickY = 0
	state.rightStickX = 0
	state.rightStickY = 0
	p.emitControllerState(instanceID, state)
}

func (p *gstreamerPlayer) handleControllerButtonEvent(event *sdl.ControllerButtonEvent) {
	state := p.ensureControllerState(uint32(event.Which))
	mask := mapSDLControllerButton(event.Button)
	if mask == 0 {
		return
	}
	if event.State == sdl.PRESSED {
		state.buttons |= mask
		state.connected = true
	} else {
		state.buttons &^= mask
	}
	p.emitControllerState(uint32(event.Which), state)
}

func (p *gstreamerPlayer) handleControllerAxisEvent(event *sdl.ControllerAxisEvent) {
	state := p.ensureControllerState(uint32(event.Which))
	state.connected = true
	value := int16(event.Value)
	switch event.Axis {
	case sdl.CONTROLLER_AXIS_LEFTX:
		state.leftStickX = value
	case sdl.CONTROLLER_AXIS_LEFTY:
		state.leftStickY = -value
	case sdl.CONTROLLER_AXIS_RIGHTX:
		state.rightStickX = value
	case sdl.CONTROLLER_AXIS_RIGHTY:
		state.rightStickY = -value
	case sdl.CONTROLLER_AXIS_TRIGGERLEFT:
		state.leftTrigger = normalizeSDLTrigger(value)
	case sdl.CONTROLLER_AXIS_TRIGGERRIGHT:
		state.rightTrigger = normalizeSDLTrigger(value)
	}
	p.emitControllerState(uint32(event.Which), state)
}

func (p *gstreamerPlayer) emitControllerState(instanceID uint32, state *controllerState) {
	controllerID := uint8(instanceID % 4)
	_ = p.emitInput("gamepad", protocol.GamepadInput{
		ControllerID: controllerID,
		Buttons:      state.buttons,
		LeftTrigger:  state.leftTrigger,
		RightTrigger: state.rightTrigger,
		LeftStickX:   state.leftStickX,
		LeftStickY:   state.leftStickY,
		RightStickX:  state.rightStickX,
		RightStickY:  state.rightStickY,
		Connected:    state.connected,
	})
}

func (p *gstreamerPlayer) ensureControllerState(instanceID uint32) *controllerState {
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.controllers == nil {
		p.controllers = map[uint32]*controllerState{}
	}
	state := p.controllers[instanceID]
	if state == nil {
		state = &controllerState{}
		p.controllers[instanceID] = state
	}
	return state
}

func (p *gstreamerPlayer) emitInput(kind string, payload any) error {
	if p.inputSink == nil {
		return nil
	}
	raw, err := json.Marshal(payload)
	if err != nil {
		return err
	}
	return p.inputSink(protocol.InputMessage{Kind: kind, Payload: raw})
}

type keyboardMapping struct {
	keycode   uint16
	scancode  uint16
	modifiers uint16
}

func mapSDLKeyboardEvent(event *sdl.KeyboardEvent) *keyboardMapping {
	vk, sc, ok := sdlScancodeMap[event.Keysym.Scancode]
	if !ok {
		return nil
	}
	mods := uint16(0)
	if event.Keysym.Mod&sdl.KMOD_SHIFT != 0 {
		mods |= 0x01
	}
	if event.Keysym.Mod&sdl.KMOD_CTRL != 0 {
		mods |= 0x02
	}
	if event.Keysym.Mod&sdl.KMOD_ALT != 0 {
		mods |= 0x04
	}
	if event.Keysym.Mod&sdl.KMOD_GUI != 0 {
		mods |= 0x08
	}
	if event.Keysym.Mod&sdl.KMOD_CAPS != 0 {
		mods |= 0x10
	}
	if event.Keysym.Mod&sdl.KMOD_NUM != 0 {
		mods |= 0x20
	}
	return &keyboardMapping{keycode: vk, scancode: sc, modifiers: mods}
}

var sdlScancodeMap = map[sdl.Scancode][2]uint16{
	sdl.SCANCODE_A: {0x41, 0x04}, sdl.SCANCODE_B: {0x42, 0x05}, sdl.SCANCODE_C: {0x43, 0x06},
	sdl.SCANCODE_D: {0x44, 0x07}, sdl.SCANCODE_E: {0x45, 0x08}, sdl.SCANCODE_F: {0x46, 0x09},
	sdl.SCANCODE_G: {0x47, 0x0a}, sdl.SCANCODE_H: {0x48, 0x0b}, sdl.SCANCODE_I: {0x49, 0x0c},
	sdl.SCANCODE_J: {0x4a, 0x0d}, sdl.SCANCODE_K: {0x4b, 0x0e}, sdl.SCANCODE_L: {0x4c, 0x0f},
	sdl.SCANCODE_M: {0x4d, 0x10}, sdl.SCANCODE_N: {0x4e, 0x11}, sdl.SCANCODE_O: {0x4f, 0x12},
	sdl.SCANCODE_P: {0x50, 0x13}, sdl.SCANCODE_Q: {0x51, 0x14}, sdl.SCANCODE_R: {0x52, 0x15},
	sdl.SCANCODE_S: {0x53, 0x16}, sdl.SCANCODE_T: {0x54, 0x17}, sdl.SCANCODE_U: {0x55, 0x18},
	sdl.SCANCODE_V: {0x56, 0x19}, sdl.SCANCODE_W: {0x57, 0x1a}, sdl.SCANCODE_X: {0x58, 0x1b},
	sdl.SCANCODE_Y: {0x59, 0x1c}, sdl.SCANCODE_Z: {0x5a, 0x1d},
	sdl.SCANCODE_1: {0x31, 0x1e}, sdl.SCANCODE_2: {0x32, 0x1f}, sdl.SCANCODE_3: {0x33, 0x20},
	sdl.SCANCODE_4: {0x34, 0x21}, sdl.SCANCODE_5: {0x35, 0x22}, sdl.SCANCODE_6: {0x36, 0x23},
	sdl.SCANCODE_7: {0x37, 0x24}, sdl.SCANCODE_8: {0x38, 0x25}, sdl.SCANCODE_9: {0x39, 0x26},
	sdl.SCANCODE_0:      {0x30, 0x27},
	sdl.SCANCODE_RETURN: {0x0d, 0x28}, sdl.SCANCODE_ESCAPE: {0x1b, 0x29}, sdl.SCANCODE_BACKSPACE: {0x08, 0x2a},
	sdl.SCANCODE_TAB: {0x09, 0x2b}, sdl.SCANCODE_SPACE: {0x20, 0x2c}, sdl.SCANCODE_MINUS: {0xbd, 0x2d},
	sdl.SCANCODE_EQUALS: {0xbb, 0x2e}, sdl.SCANCODE_LEFTBRACKET: {0xdb, 0x2f}, sdl.SCANCODE_RIGHTBRACKET: {0xdd, 0x30},
	sdl.SCANCODE_BACKSLASH: {0xdc, 0x31}, sdl.SCANCODE_SEMICOLON: {0xba, 0x33}, sdl.SCANCODE_APOSTROPHE: {0xde, 0x34},
	sdl.SCANCODE_GRAVE: {0xc0, 0x35}, sdl.SCANCODE_COMMA: {0xbc, 0x36}, sdl.SCANCODE_PERIOD: {0xbe, 0x37},
	sdl.SCANCODE_SLASH: {0xbf, 0x38},
	sdl.SCANCODE_F1:    {0x70, 0x3a}, sdl.SCANCODE_F2: {0x71, 0x3b}, sdl.SCANCODE_F3: {0x72, 0x3c},
	sdl.SCANCODE_F4: {0x73, 0x3d}, sdl.SCANCODE_F5: {0x74, 0x3e}, sdl.SCANCODE_F6: {0x75, 0x3f},
	sdl.SCANCODE_F7: {0x76, 0x40}, sdl.SCANCODE_F8: {0x77, 0x41}, sdl.SCANCODE_F9: {0x78, 0x42},
	sdl.SCANCODE_F10: {0x79, 0x43}, sdl.SCANCODE_F11: {0x7a, 0x44}, sdl.SCANCODE_F12: {0x7b, 0x45},
	sdl.SCANCODE_INSERT: {0x2d, 0x49}, sdl.SCANCODE_HOME: {0x24, 0x4a}, sdl.SCANCODE_PAGEUP: {0x21, 0x4b},
	sdl.SCANCODE_DELETE: {0x2e, 0x4c}, sdl.SCANCODE_END: {0x23, 0x4d}, sdl.SCANCODE_PAGEDOWN: {0x22, 0x4e},
	sdl.SCANCODE_RIGHT: {0x27, 0x4f}, sdl.SCANCODE_LEFT: {0x25, 0x50}, sdl.SCANCODE_DOWN: {0x28, 0x51}, sdl.SCANCODE_UP: {0x26, 0x52},
}

func mapSDLMouseButton(button uint8) uint8 {
	switch button {
	case sdl.BUTTON_LEFT:
		return 1
	case sdl.BUTTON_MIDDLE:
		return 2
	case sdl.BUTTON_RIGHT:
		return 3
	case sdl.BUTTON_X1:
		return 4
	case sdl.BUTTON_X2:
		return 5
	default:
		return 0
	}
}

func mapSDLControllerButton(button uint8) uint16 {
	switch button {
	case sdl.CONTROLLER_BUTTON_A:
		return gamepadA
	case sdl.CONTROLLER_BUTTON_B:
		return gamepadB
	case sdl.CONTROLLER_BUTTON_X:
		return gamepadX
	case sdl.CONTROLLER_BUTTON_Y:
		return gamepadY
	case sdl.CONTROLLER_BUTTON_BACK:
		return gamepadBack
	case sdl.CONTROLLER_BUTTON_GUIDE:
		return gamepadGuide
	case sdl.CONTROLLER_BUTTON_START:
		return gamepadStart
	case sdl.CONTROLLER_BUTTON_LEFTSTICK:
		return gamepadLS
	case sdl.CONTROLLER_BUTTON_RIGHTSTICK:
		return gamepadRS
	case sdl.CONTROLLER_BUTTON_LEFTSHOULDER:
		return gamepadLB
	case sdl.CONTROLLER_BUTTON_RIGHTSHOULDER:
		return gamepadRB
	case sdl.CONTROLLER_BUTTON_DPAD_UP:
		return gamepadDpadUp
	case sdl.CONTROLLER_BUTTON_DPAD_DOWN:
		return gamepadDpadDown
	case sdl.CONTROLLER_BUTTON_DPAD_LEFT:
		return gamepadDpadLeft
	case sdl.CONTROLLER_BUTTON_DPAD_RIGHT:
		return gamepadDpadRight
	default:
		return 0
	}
}

func normalizeSDLTrigger(value int16) uint8 {
	normalized := (float64(value) + 32768.0) / 65535.0
	return uint8(math.Round(math.Max(0, math.Min(255, normalized*255))))
}

func clampInt16(value int) int16 {
	if value > math.MaxInt16 {
		return math.MaxInt16
	}
	if value < math.MinInt16 {
		return math.MinInt16
	}
	return int16(value)
}
