import { useState, useEffect } from "react";
import {
  Box,
  Grid,
  Stack,
  GridItem,
} from "@chakra-ui/react";
import { FaFileVideo, FaFileCsv, FaFileImport } from "react-icons/fa";
import * as tauri from 'tauri/api/tauri';
import * as dialog from 'tauri/api/dialog';

import IButton from "../components/Button";
import IIConButton from "../components/IconButton";
import IInput from "../components/Input";
import VideoCanvas from "../components/VideoCanvas";
import Daq from "../components/Daq";
import ITag from "../components/Tag";

function BasicSettings({ config, setConfig, awsl }) {
  const [frameIndex, setFrameIndex] = useState(0);
  const [scrollToColumn, setScrollToColumn] = useState(-1);
  const [scrollToRow, setScrollToRow] = useState(-1);

  useEffect(() => { if (config === "") loadDefaultConfig() }, []);

  function loadConfig() {
    dialog.open({ filter: "json" }).then(path => {
      tauri.promisified({
        cmd: "loadConfig",
        body: { String: path },
      })
        .then(ok => setConfig(ok))
        .catch(err => awsl(err));
    });
  }

  function loadDefaultConfig() {
    tauri.promisified({ cmd: "loadDefaultConfig" })
      .then(ok => setConfig(ok))
      .catch(err => awsl(err));
  }

  function saveConfig() {
    if (config.save_dir === "") {
      awsl("请先确定保存根目录");
      return;
    }
    tauri.promisified({ cmd: "saveConfig" })
      .catch(err => awsl(err));
  }

  function setSaveDir() {
    dialog.open({ directory: true }).then(save_dir => {
      tauri.promisified({
        cmd: "setSaveDir",
        body: { String: save_dir },
      })
        .then(ok => setConfig(ok))
        .catch(err => awsl(err));
    });
  }

  function setVideoPath() {
    dialog.open({
      filter: "avi,mp4,mkv",
      defaultPath: config.video_path.substr(0, config.video_path.lastIndexOf("\\") + 1)
    })
      .then(video_path => {
        if (video_path === config.video_path) return;
        tauri.promisified({
          cmd: "setVideoPath",
          body: { String: video_path },
        })
          .then(ok => { awsl(""); setConfig(ok); })
          .catch(err => awsl(err));
      });
  }

  function setDaqPath() {
    dialog.open({
      filter: "lvm,xlsx",
      defaultPath: config.daq_path.substr(0, config.daq_path.lastIndexOf("\\") + 1)
    })
      .then(daq_path => {
        if (daq_path === config.daq_path) return;
        tauri.promisified({
          cmd: "setDaqPath",
          body: { String: daq_path },
        })
          .then(ok => { awsl(""); setConfig(ok); })
          .catch(err => awsl(err));
      });
  }

  function setStartFrame(start_frame) {
    if (start_frame === config.start_frame) return;
    if (start_frame < 0) {
      awsl("帧数须为正值");
      return;
    }
    tauri.promisified({
      cmd: "setStartFrame",
      body: { Uint: start_frame },
    })
      .then(ok => setConfig(ok))
      .catch(err => awsl(err));
  }

  function setStartRow(start_row) {
    if (start_row === config.start_row) return;
    if (start_row < 0) {
      awsl("行数须为正值");
      return;
    }
    tauri.promisified({
      cmd: "setStartRow",
      body: { Uint: start_row },
    })
      .then(ok => setConfig(ok))
      .catch(err => awsl(err));
  }

  function synchronize() {
    if (frameIndex < 0 || scrollToRow < 0) {
      awsl("未选中数据行");
      return;
    }
    tauri.promisified({
      cmd: "synchronize",
      body: { UintVec: [frameIndex, scrollToRow] },
    })
      .then(ok => setConfig(ok))
      .catch(err => awsl(err));
  }

  return (
    <Stack>
      <Grid templateColumns="repeat(12, 1fr)" gap={2} marginX="25px">
        <GridItem colSpan={1}>
          <Stack spacing="5px">
            <IButton text="重置配置" onClick={loadDefaultConfig} hover="重置为您上一次保存的配置" />
            <IButton text="导入配置" onClick={loadConfig} />
            <IButton text="保存配置" onClick={saveConfig} />
          </Stack>
        </GridItem>
        <GridItem colSpan={8}>
          <Stack spacing="5px">
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
              element={<IIConButton icon={<FaFileCsv />} onClick={setDaqPath} />}
            />
          </Stack>
        </GridItem>
        <GridItem colSpan={3}>
          <Stack spacing="5px">
            <IInput
              leftTag="起始帧数"
              value={config.frame_num > 0 ? config.start_frame + 1 : ""}
              mutable
              onBlur={v => setStartFrame(parseInt(v) - 1)}
              rightTag={config.frame_num > 0 ?
                `[${config.start_frame + 1}, 
                  ${Math.min(config.start_frame + config.frame_num, config.total_frames)}] 
                / ${config.total_frames}` : ""}
            />
            <IInput
              leftTag="起始行数"
              value={config.frame_num > 0 ? config.start_row + 1 : ""}
              onBlur={v => setStartRow(parseInt(v) - 1)}
              mutable
              rightTag={config.frame_num > 0 ?
                `[${config.start_row + 1}, 
                  ${Math.min(config.start_row + config.frame_num, config.total_rows)}] 
                / ${config.total_rows}` : ""}
            />
            <IInput
              leftTag="帧率"
              value={config.frame_rate > 0 ? config.frame_rate : ""}
              rightTag="Hz"
            />
          </Stack>
        </GridItem>
      </Grid>
      <Grid
        templateRows="repeat(13, 1fr)"
        templateColumns="repeat(12, 1fr)"
        gap={2}
        marginX="25px"
      >
        <GridItem rowSpan={13}>
          <VideoCanvas
            frameIndex={frameIndex}
            setFrameIndex={setFrameIndex}
            config={config}
            setConfig={setConfig}
            awsl={awsl}
          />
        </GridItem>
        <GridItem rowSpan={1} colSpan={1}>
          <Box marginTop="5px">
            <ITag text={`行数：${scrollToRow >= 0 ? scrollToRow + 1 : 0}`} w="100px" />
          </Box>
        </GridItem>
        <GridItem rowSpan={1} colSpan={1}>
          <Box marginTop="5px">
            <ITag text={`列数：${scrollToColumn >= 0 ? scrollToColumn + 1 : 0}`} w="100px" />
          </Box>
        </GridItem>
        <GridItem rowSpan={1} colSpan={2}>
          <IButton
            text="确认同步"
            onClick={synchronize}
            hover="确认视频当前帧数与表格选中行数对应同一时刻"
            size="sm"
          />
        </GridItem>
        <GridItem rowSpan={1} colSpan={7}></GridItem>
        <GridItem rowSpan={7} colSpan={11}>
          <Daq
            config={config}
            awsl={awsl}
            scrollToColumn={scrollToColumn}
            setScrollToColumn={setScrollToColumn}
            scrollToRow={scrollToRow}
            setScrollToRow={setScrollToRow}
          />
        </GridItem>
      </Grid>
    </Stack >
  )
}

export default BasicSettings
