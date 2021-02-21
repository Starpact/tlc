import { useState, useEffect } from "react"
import {
  Button,
  HStack,
  Slider,
  SliderTrack,
  SliderFilledTrack,
  SliderThumb,
  Tag,
  Text,
  Image,
  Grid,
  Stack,
  GridItem
} from "@chakra-ui/react"
import { FaFileVideo, FaFileCsv, FaFileImport } from "react-icons/fa"
import * as tauri from 'tauri/api/tauri'
import * as dialog from 'tauri/api/dialog'

import IButton from "../components/Button"
import IIConButton from "../components/IconButton"
import IInput from "../components/Input"
import Canvas from "../components/Canvas"

function BasicSettings({ config, setConfig, setErrMsg, loadDefaultConfig }) {
  const [frame, setFrame] = useState("");
  const [frameIndex, setFrameIndex] = useState(1);

  useEffect(() => getFrame(0), []);

  function loadConfig() {
    dialog.open({ filter: "json" }).then(path => {
      tauri.promisified({
        cmd: "LoadConfig",
        config_path: path,
      })
        .then(ok => setConfig(Uint8Array(ok)))
        .catch(err => setErrMsg(err));
    });
  }

  function saveConfig() {
    if (config.save_dir === "") {
      setErrMsg("请先确定保存根目录");
      return;
    }
    tauri.promisified({ cmd: "SaveConfig" })
      .catch(err => setErrMsg(err));
  }

  function setSaveDir() {
    dialog.open({ directory: true }).then(save_dir => {
      tauri.promisified({
        cmd: "SetSaveDir",
        save_dir,
      })
        .then(ok => setConfig(ok))
        .catch(err => setErrMsg(err));
    });
  }

  function setVideoPath() {
    dialog.open({
      filter: "avi,mp4,mkv",
      defaultPath: config.video_path.substr(0, config.video_path.lastIndexOf("\\") + 1)
    })
      .then(video_path => {
        tauri.promisified({
          cmd: "SetVideoPath",
          video_path,
        })
          .then(ok => setConfig(ok))
          .catch(err => setErrMsg(err));
      });
  }

  function setDAQPath() {
    dialog.open({
      filter: "lvm,xlsx",
      defaultPath: config.daq_path.substr(0, config.daq_path.lastIndexOf("\\") + 1)
    })
      .then(daq_path => {
        tauri.promisified({
          cmd: "SetDAQPath",
          daq_path,
        })
          .then(ok => setConfig(ok))
          .catch(err => setErrMsg(err));
      });
  }

  function setStartFrame(start_frame) {
    if (start_frame === config.start_frame) return;
    tauri.promisified({
      cmd: "SetStartFrame",
      start_frame: start_frame,
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function setStartRow(start_row) {
    if (start_row === config.start_row) return;
    tauri.promisified({
      cmd: "SetStartRow",
      start_row,
    })
      .then(ok => setConfig(ok))
      .catch(err => setErrMsg(err));
  }

  function getFrame(frame_index) {
    tauri.promisified({
      cmd: "GetFrame",
      frame_index,
    })
      .then(ok => setFrame(ok))
      .catch(err => setErrMsg(err));
  }

  return (
    <Stack>
      <Grid templateColumns="repeat(4, 1fr)" gap={2} marginX="30px">
        <GridItem colSpan={1}>
          <Stack spacing="10px">
            <IButton text="重置配置" onClick={loadDefaultConfig} hover="重置为您上一次保存的配置" />
            <IButton text="导入配置" onClick={loadConfig} />
            <IButton text="保存配置" onClick={saveConfig} />
          </Stack>
        </GridItem>
        <GridItem colSpan={2}>
          <Stack spacing="10px">
            <IInput
              leftTag="保存根目录"
              hover="所有结果的保存根目录，该目录下将自动创建config、data和plots子目录分类保存处理结果"
              placeholder="保存所有结果的根目录"
              value={config.save_dir}
              element={<IIConButton icon={<FaFileImport />} onClick={setSaveDir} />}
            />
            <IInput
              leftTag="视频文件路径"
              value={config.video_path}
              element={<IIConButton icon={<FaFileVideo />} onClick={setVideoPath} />}
            />
            <IInput
              leftTag="数采文件路径"
              value={config.daq_path}
              element={<IIConButton icon={<FaFileCsv />} onClick={setDAQPath} />}
            />
          </Stack>
        </GridItem>
        <GridItem colSpan={1}>
          <Stack spacing="10px">
            <IInput
              leftTag="起始帧数"
              value={config.frame_num > 0 ? config.start_frame : ""}
              mutable
              onBlur={v => setStartFrame(parseInt(v))}
              rightTag={config.frame_num > 0 ?
                `[${config.start_frame}, ${config.start_frame + config.frame_num}] / ${config.total_frames}` : ""}
            />
            <IInput
              leftTag="起始行数"
              value={config.frame_num > 0 ? config.start_row : ""}
              onBlur={v => setStartRow(parseInt(v))}
              mutable
              rightTag={config.frame_num > 0 ?
                `[${config.start_row}, ${config.start_row + config.frame_num}] / ${config.total_rows}` : ""}
            />
            <IInput
              leftTag="帧率"
              value={config.frame_rate > 0 ? config.frame_rate : ""}
              rightTag="Hz"
            />
          </Stack>
        </GridItem>
      </Grid>
      <HStack>
        <Stack>
          {/* <Image src={`data:image/png;base64,${frame}`} htmlWidth="640" htmlHeight="512" /> */}
          <Canvas draw={ctx => {
            const imgData = ctx.createImageData(1280 / 2, 1024 / 2);
            let k = 0;
            for (let i = 0; i < imgData.data.length; i += 4) {
              imgData.data[i] = frame[k++];
              imgData.data[i + 1] = frame[k++];
              imgData.data[i + 2] = frame[k++];
              imgData.data[i + 3] = 255;
            }
            ctx.putImageData(imgData, 0, 0);
          }} />
          {/* <Button onClick={() => getFrame(0)}></Button> */}
          <HStack>
            <Tag size="lg" bgColor="#98971a">
              <Text color="#32302f" fontWeight="bold">
                {frameIndex}
              </Text>
            </Tag>
            <Slider
              defaultValue={1}
              min={1}
              max={config.total_frames}
              onChange={v => getFrame(parseInt(v) - 1)}
              onChangeEnd={v => setFrameIndex(parseInt(v))}
            >
              <SliderTrack bgColor="#665c54">
                <SliderFilledTrack bgColor="#98971a" />
              </SliderTrack>
              <SliderThumb bgColor="#928374" />
            </Slider>
          </HStack>
        </Stack>
        <Stack>
          <IInput
            leftTag="计算区域左上角Y"
            hover="与上边界的距离"
            value={!!config.top_left_pos && config.top_left_pos[0]}
          />
          <IInput
            leftTag="计算区域左上角X"
            hover="与左边界的距离"
            value={!!config.top_left_pos && config.top_left_pos[1]}
          />
          <IInput
            leftTag="计算区域高度"
            value={!!config.region_shape && config.region_shape[0]}
          />
          <IInput
            leftTag="计算区域宽度"
            value={!!config.region_shape && config.region_shape[1]}
          />
        </Stack>
      </HStack>
    </Stack>
  )
}

export default BasicSettings
