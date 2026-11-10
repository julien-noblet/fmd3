function getinfo()
	MANGAINFO.URL = MaybeFillHost(MODULE.RootURL, URL)
	HTTP.Cookies.Values['isAdult'] = '1'
	if HTTP.GET(MANGAINFO.URL) then
		local x = CreateTXQuery(HTTP.Document)
		if MANGAINFO.Title == '' then
			MANGAINFO.Title=x.XPathString('//div[@class="banner_detail_form"]//p[@class="title"]/text()')
		end
		MANGAINFO.CoverLink=MaybeFillHost(MODULE.RootURL,x.XPathString('//div[@class="cover"]/img/@src'))
		MANGAINFO.Authors=x.XPathStringAll('//div[@class="info"]/p[@class="subtitle"]/a')
		MANGAINFO.Genres=x.XPathStringAll('//div[@class="info"]/p[@class="tip"]/span[contains(., "题材")]/a')
		MANGAINFO.Status = MangaInfoStatusIfPos(x.XPathString('//div[@class="info"]/p[@class="tip"]/span[contains(., "状态")]/span'), '连载中', '已完结');
		MANGAINFO.Summary=x.XPathString('//div[@class="info"]/p[@class="content"]')
		v=x.XPath('//div[@id="chapterlistload"]/ul')
		for i = 1, v.Count do
			local v1 = v.Get(i)
			local w = x.XPath('.//li/a', v1)
			for j = 1, w.Count do
				local w1 = w.Get(j)
				MANGAINFO.ChapterLinks.Add(MODULE.RootURL .. w1.GetAttribute('href'))
				MANGAINFO.ChapterNames.Add(w1.ToString())
			end
		end
		MANGAINFO.ChapterLinks.Reverse(); MANGAINFO.ChapterNames.Reverse()
		return no_error
	else
		return net_problem
	end
end

function urlencode(str)
	if (str) then
		str = string.gsub (str, "\n", "\r\n")
		str = string.gsub (str, "([^%w ])",
			 function (c) return string.format ("%%%02X", string.byte(c)) end)
		str = string.gsub (str, " ", "+")
	end
	return str
end

local js = require 'utils.jsunpack'
function gettext(s)
	s = SeparateRight(s, "}('")
	local text = SeparateLeft(s, "',")
	local a = tonumber(GetBetween("',", ",", s))
	s = SeparateRight(s, "',")
	local c = tonumber(GetBetween(",", ",'", s))
	local w = js.splitstr(GetBetween(",'", "'", s), '|')
	return js.unpack36(text, a, c, w)
end

function getpagenumber()
	local u = MaybeFillHost(MODULE.RootURL,URL)
	if HTTP.GET(u) then
		local x=CreateTXQuery(HTTP.Document)
		local s = x.XPathString('//script[contains(., "DM5_MID")]')
		local dm5mid = GetBetween('DM5_MID=', ';', s)
		local dm5cid = GetBetween('DM5_CID=', ';', s)
		local dm5viewsign = GetBetween('DM5_VIEWSIGN="', '";', s)
		local dm5viewsigndt = GetBetween('DM5_VIEWSIGN_DT="', '";', s)
		local dm5key = x.XPathString('//*[@id="dm5_key"]/@value')
		local dm5page, cnt = 1, 0

		local total = tonumber(x.XPathString('(//div[@id="chapterpager"])[1]/a[last()]'))
		if total == nil then total = 0 end

		while (cnt < total) and (dm5page <= total) do
			local xhrurl = string.format('%s/chapterfun.ashx?cid=%s&page=%d&key=%s&language=1&gtk=6&_cid=%s&_mid=%s&_dt=%s&_sign=%s',
				u:gsub('/+$', ''), dm5cid, dm5page, dm5key, dm5cid, dm5mid, urlencode(dm5viewsigndt), dm5viewsign)
			if HTTP.Terminated then break end
			HTTP.Reset()
			HTTP.Headers.Values['Referer'] = u
			if HTTP.XHR(xhrurl) then
				local s = gettext(HTTP.Document.ToString())
				local cid = GetBetween('cid=', ';', s)
				local key = GetBetween("key=\\'", "\\';", s)
				local pix = GetBetween('pix="', '";', s)
				local pvalue = '[' .. GetBetween('pvalue=[', '];', s) .. ']'
				x.ParseHTML(pvalue)
				local v = x.XPath('json(*)()')
				for i = 1, v.Count do
					local v1 = v.Get(i)
					TASK.PageLinks.Add(pix .. v1.ToString() .. string.format('?cid=%s&key=%s', cid, key))
					cnt = cnt + 1
				end
				dm5page = dm5page + v.Count
			else
				return false
			end
		end
		TASK.PageContainerLinks.Text = u
		return true
	else
		return false
	end
end

function BeforeDownloadImage()
	HTTP.Headers.Values['Referer'] = TASK.PageContainerLinks.Text
	return true
end

local perpage = 100
function getdirectorypagenumber()
	local u = string.format('%s/dm5.ashx?t=%d', MODULE.RootURL, os.time()*1000)
	local data = 'pagesize=1&pageindex=1&tagid=0&areaid=0&status=0&usergroup=0&pay=-1&char=&sort=18&action=getclasscomics'
	if HTTP.POST(u, data) then
		local x = CreateTXQuery(HTTP.Document)
		PAGENUMBER = tonumber(x.XPathString('json(*).Count')) or 1
	if PAGENUMBER > 1 then
			PAGENUMBER = math.ceil(PAGENUMBER / perpage)
		end
		return no_error
	else
		return net_problem
	end
end

function getnameandlink()
	local u = string.format('%s/dm5.ashx?t=%d', MODULE.RootURL, os.time()*1000)
	local data = string.format('pagesize=%d&pageindex=%d&tagid=0&areaid=0&status=0&usergroup=0&pay=-1&char=&sort=18&action=getclasscomics', perpage, (URL + 1))
	if HTTP.POST(u, data) then
		local x = CreateTXQuery(HTTP.Document)
		local v = x.XPath('json(*).UpdateComicItems()')
		for i = 1, v.Count do
			local v1 = v.Get(i)
			LINKS.Add(MODULE.RootURL .. '/' .. x.XPathString('UrlKey', v1) .. '/')
			NAMES.Add(x.XPathString('Title', v1))
		end
		return no_error
	else
		return net_problem
	end
end

function Init()
	local m = NewWebsiteModule()
	m.ID                       = 'dc319368a5d346dc876711cebc562066'
	m.Category                 = 'Raw'
	m.Name                     = 'DM5'
	m.RootURL                  = 'http://www.dm5.com'
	m.LastUpdated              = 'May 23, 2018'
	m.OnGetInfo                = 'getinfo'
	m.OnGetPageNumber          = 'getpagenumber'
	m.OnGetDirectoryPageNumber = 'getdirectorypagenumber'
	m.OnGetNameAndLink         = 'getnameandlink'
	m.OnBeforeDownloadImage    = 'BeforeDownloadImage'
	m.SortedList               = true
end
